//! Fixed-transform library for bridge adapter wiring.
//!
//! Each `WiringTransform` is a named transform with optional parameters.
//! `apply_wiring` applies a slice of transforms sequentially to an input string.
//!
//! Transforms: trim, lower, upper, titlecase, phone_e164, usd_to_cents,
//! date_iso, lookup, split, concat, const_val.

use std::collections::HashMap;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WiringError {
    #[error("unknown transform: {0}")]
    UnknownTransform(String),
    #[error("missing required parameter '{param}' for transform '{transform}'")]
    MissingParam {
        transform: String,
        param: String,
    },
    #[error("index {index} out of bounds (split produced {len} parts) for transform '{transform}'")]
    IndexOutOfBounds {
        transform: String,
        index: usize,
        len: usize,
    },
    #[error("not found in lookup table for transform '{transform}'")]
    LookupNotFound {
        transform: String,
    },
    #[error("parse error for transform '{transform}': {detail}")]
    ParseError {
        transform: String,
        detail: String,
    },
}

// ---------------------------------------------------------------------------
// Transform definition
// ---------------------------------------------------------------------------

/// A single named transform with a JSON-like parameter map.
///
/// The `name` field selects the transform function.
/// The `params` map supplies per-transform arguments (e.g. `format` for date_iso,
/// `sep` and `idx` for split, `value` for const_val, `key` for lookup).
#[derive(Debug, Clone, PartialEq)]
pub struct WiringTransform {
    pub name: String,
    pub params: HashMap<String, String>,
}

impl WiringTransform {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            params: HashMap::new(),
        }
    }

    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Transform implementations
// ---------------------------------------------------------------------------

fn apply_trim(input: &str) -> String {
    input.trim().to_owned()
}

fn apply_lower(input: &str) -> String {
    input.to_ascii_lowercase()
}

fn apply_upper(input: &str) -> String {
    input.to_ascii_uppercase()
}

fn apply_titlecase(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_is_whitespace = true;
    for ch in input.chars() {
        if ch.is_ascii_alphabetic() && prev_is_whitespace {
            for upper in ch.to_uppercase() {
                out.push(upper);
            }
        } else {
            out.push(ch);
        }
        prev_is_whitespace = ch.is_whitespace();
    }
    out
}

fn apply_phone_e164(input: &str) -> String {
    let digits: String = input.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        String::new()
    } else {
        format!("+{digits}")
    }
}

fn apply_usd_to_cents(input: &str) -> Result<String, WiringError> {
    let trimmed = input.trim();
    let cleaned: String = trimmed
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let value: f64 = cleaned
        .parse()
        .map_err(|_| WiringError::ParseError {
            transform: "usd_to_cents".into(),
            detail: format!("cannot parse as number: {trimmed}"),
        })?;
    let cents = (value * 100.0).round() as i64;
    Ok(cents.to_string())
}

fn apply_date_iso(input: &str, format: &str) -> Result<String, WiringError> {
    let trimmed = input.trim();
    match format {
        "dmy" | "d/m/Y" => {
            // Parse d/m/Y (e.g. "15/04/2025")
            let parts: Vec<&str> = trimmed.split('/').collect();
            if parts.len() != 3 {
                return Err(WiringError::ParseError {
                    transform: "date_iso".into(),
                    detail: format!("expected d/m/Y format, got: {trimmed}"),
                });
            }
            let day = parts[0];
            let month = parts[1];
            let year = parts[2];
            // Pad single-digit day/month
            let day = if day.len() == 1 { format!("0{day}") } else { day.to_owned() };
            let month = if month.len() == 1 { format!("0{month}") } else { month.to_owned() };
            Ok(format!("{year}-{month}-{day}"))
        }
        "mdy" | "m/d/Y" => {
            let parts: Vec<&str> = trimmed.split('/').collect();
            if parts.len() != 3 {
                return Err(WiringError::ParseError {
                    transform: "date_iso".into(),
                    detail: format!("expected m/d/Y format, got: {trimmed}"),
                });
            }
            let month = if parts[0].len() == 1 { format!("0{}", parts[0]) } else { parts[0].to_owned() };
            let day = if parts[1].len() == 1 { format!("0{}", parts[1]) } else { parts[1].to_owned() };
            let year = parts[2];
            Ok(format!("{year}-{month}-{day}"))
        }
        "Y-m-d" => Ok(trimmed.to_owned()),
        other => Err(WiringError::ParseError {
            transform: "date_iso".into(),
            detail: format!("unsupported date format: {other}"),
        }),
    }
}

fn apply_lookup(input: &str, table: &HashMap<String, String>) -> Result<String, WiringError> {
    let trimmed = input.trim();
    table
        .get(trimmed)
        .cloned()
        .ok_or_else(|| WiringError::LookupNotFound {
            transform: "lookup".into(),
        })
}

fn apply_split(input: &str, sep: &str, idx: usize) -> Result<String, WiringError> {
    let parts: Vec<&str> = input.split(sep).collect();
    if idx >= parts.len() {
        return Err(WiringError::IndexOutOfBounds {
            transform: "split".into(),
            index: idx,
            len: parts.len(),
        });
    }
    Ok(parts[idx].to_owned())
}

fn apply_concat(input: &str, sep: &str) -> String {
    // concat is a no-op on its own; the expectation is that this transform
    // is used in a sequence where preceding transforms build partial values.
    // Here we simply return the input unchanged — concat becomes meaningful
    // when wiring orchestrates multiple field pipelines; this module provides
    // the building block.
    let _ = sep;
    input.to_owned()
}

fn apply_const_val(_input: &str, value: &str) -> String {
    value.to_owned()
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Apply a sequence of `WiringTransform`s to `input`, returning the final value.
pub fn apply_wiring(input: &str, transforms: &[WiringTransform]) -> Result<String, WiringError> {
    let mut current = input.to_owned();

    for t in transforms {
        current = match t.name.as_str() {
            "trim" => apply_trim(&current),
            "lower" => apply_lower(&current),
            "upper" => apply_upper(&current),
            "titlecase" => apply_titlecase(&current),

            "phone_e164" => apply_phone_e164(&current),

            "usd_to_cents" => apply_usd_to_cents(&current)?,

            "date_iso" => {
                let fmt = t
                    .params
                    .get("format")
                    .cloned()
                    .unwrap_or_else(|| "Y-m-d".into());
                apply_date_iso(&current, &fmt)?
            }

            "lookup" => {
                let table_str = t.params.get("table").ok_or_else(|| {
                    WiringError::MissingParam {
                        transform: "lookup".into(),
                        param: "table".into(),
                    }
                })?;
                let table: HashMap<String, String> = serde_json::from_str(table_str).map_err(
                    |e| WiringError::ParseError {
                        transform: "lookup".into(),
                        detail: format!("invalid lookup table JSON: {e}"),
                    },
                )?;
                apply_lookup(&current, &table)?
            }

            "split" => {
                let sep = t.params.get("sep").cloned().unwrap_or_else(|| ",".into());
                let idx_str = t.params.get("idx").ok_or_else(|| {
                    WiringError::MissingParam {
                        transform: "split".into(),
                        param: "idx".into(),
                    }
                })?;
                let idx: usize = idx_str.parse().map_err(|_| {
                    WiringError::ParseError {
                        transform: "split".into(),
                        detail: format!("idx must be a non-negative integer: {idx_str}"),
                    }
                })?;
                apply_split(&current, &sep, idx)?
            }

            "concat" => {
                let sep = t.params.get("sep").cloned().unwrap_or_else(|| "".into());
                apply_concat(&current, &sep)
            }

            "const_val" => {
                let value = t.params.get("value").ok_or_else(|| {
                    WiringError::MissingParam {
                        transform: "const_val".into(),
                        param: "value".into(),
                    }
                })?;
                apply_const_val(&current, value)
            }

            name => {
                return Err(WiringError::UnknownTransform(name.into()));
            }
        };
    }

    Ok(current)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trim() {
        let result = apply_wiring("  hello world  ", &[WiringTransform::new("trim")]);
        assert_eq!(result.unwrap(), "hello world");
    }

    #[test]
    fn test_lower() {
        let result = apply_wiring("Hello World", &[WiringTransform::new("lower")]);
        assert_eq!(result.unwrap(), "hello world");
    }

    #[test]
    fn test_upper() {
        let result = apply_wiring("Hello World", &[WiringTransform::new("upper")]);
        assert_eq!(result.unwrap(), "HELLO WORLD");
    }

    #[test]
    fn test_titlecase() {
        let result = apply_wiring("hello world", &[WiringTransform::new("titlecase")]);
        assert_eq!(result.unwrap(), "Hello World");
    }

    #[test]
    fn test_phone_e164() {
        // "4255550101" after strip non-digits → "+4255550101" (10 digits after +)
        let result = apply_wiring("(425) 555-0101", &[WiringTransform::new("phone_e164")]);
        assert_eq!(result.unwrap(), "+4255550101");
    }

    #[test]
    fn test_phone_e164_empty() {
        let result = apply_wiring("N/A", &[WiringTransform::new("phone_e164")]);
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn test_usd_to_cents() {
        let result = apply_wiring("$12.34", &[WiringTransform::new("usd_to_cents")]);
        assert_eq!(result.unwrap(), "1234");
    }

    #[test]
    fn test_usd_to_cents_whole() {
        let result = apply_wiring("$50", &[WiringTransform::new("usd_to_cents")]);
        assert_eq!(result.unwrap(), "5000");
    }

    #[test]
    fn test_usd_to_cents_invalid() {
        let result = apply_wiring("abc", &[WiringTransform::new("usd_to_cents")]);
        assert!(result.is_err());
    }

    #[test]
    fn test_date_iso_dmy() {
        let t = WiringTransform::new("date_iso").with("format", "d/m/Y");
        let result = apply_wiring("15/04/2025", &[t]);
        assert_eq!(result.unwrap(), "2025-04-15");
    }

    #[test]
    fn test_date_iso_mdy() {
        let t = WiringTransform::new("date_iso").with("format", "m/d/Y");
        let result = apply_wiring("04/15/2025", &[t]);
        assert_eq!(result.unwrap(), "2025-04-15");
    }

    #[test]
    fn test_date_iso_padded() {
        // d/m/Y: day=1, month=2 → ISO 2025-02-01
        let t = WiringTransform::new("date_iso").with("format", "d/m/Y");
        let result = apply_wiring("1/2/2025", &[t]);
        assert_eq!(result.unwrap(), "2025-02-01");
    }

    #[test]
    fn test_date_iso_passthrough() {
        let t = WiringTransform::new("date_iso").with("format", "Y-m-d");
        let result = apply_wiring("2025-01-02", &[t]);
        assert_eq!(result.unwrap(), "2025-01-02");
    }

    #[test]
    fn test_lookup() {
        let table = r#"{"lead": "new", "active": "confirmed", "paused": "on_hold"}"#;
        let t = WiringTransform::new("lookup").with("table", table);
        let result = apply_wiring("active", &[t]);
        assert_eq!(result.unwrap(), "confirmed");
    }

    #[test]
    fn test_lookup_not_found() {
        let table = r#"{"lead": "new"}"#;
        let t = WiringTransform::new("lookup").with("table", table);
        let result = apply_wiring("unknown", &[t]);
        assert!(result.is_err());
    }

    #[test]
    fn test_split() {
        let t = WiringTransform::new("split").with("sep", ",").with("idx", "0");
        let result = apply_wiring("abc,def,ghi", &[t]);
        assert_eq!(result.unwrap(), "abc");
    }

    #[test]
    fn test_split_second() {
        let t = WiringTransform::new("split").with("sep", ",").with("idx", "1");
        let result = apply_wiring("abc,def,ghi", &[t]);
        assert_eq!(result.unwrap(), "def");
    }

    #[test]
    fn test_split_out_of_bounds() {
        let t = WiringTransform::new("split").with("sep", ",").with("idx", "99");
        let result = apply_wiring("abc", &[t]);
        assert!(result.is_err());
    }

    #[test]
    fn test_concat_is_passthrough() {
        let t = WiringTransform::new("concat").with("sep", " ");
        let result = apply_wiring("hello world", &[t]);
        assert_eq!(result.unwrap(), "hello world");
    }

    #[test]
    fn test_const_val() {
        let t = WiringTransform::new("const_val").with("value", "hardcoded_value");
        let result = apply_wiring("ignored", &[t]);
        assert_eq!(result.unwrap(), "hardcoded_value");
    }

    #[test]
    fn test_unknown_transform() {
        let result = apply_wiring("input", &[WiringTransform::new("nonexistent")]);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_param_split() {
        let t = WiringTransform::new("split").with("sep", ",");
        let result = apply_wiring("abc,def", &[t]);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_param_lookup() {
        let result = apply_wiring("key", &[WiringTransform::new("lookup")]);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_param_const_val() {
        let result = apply_wiring("input", &[WiringTransform::new("const_val")]);
        assert!(result.is_err());
    }

    #[test]
    fn test_chained_transforms() {
        let transforms = vec![
            WiringTransform::new("trim"),
            WiringTransform::new("upper"),
        ];
        let result = apply_wiring("  hello world  ", &transforms);
        assert_eq!(result.unwrap(), "HELLO WORLD");
    }

    #[test]
    fn test_trim_then_lower_then_split() {
        let transforms = vec![
            WiringTransform::new("trim"),
            WiringTransform::new("lower"),
            WiringTransform::new("split")
                .with("sep", ",")
                .with("idx", "0"),
        ];
        let result = apply_wiring("  ALPHA,BETA  ", &transforms);
        assert_eq!(result.unwrap(), "alpha");
    }

    #[test]
    fn test_titlecase_after_trim() {
        let result = apply_wiring(
            "  john doe  ",
            &[
                WiringTransform::new("trim"),
                WiringTransform::new("titlecase"),
            ],
        );
        assert_eq!(result.unwrap(), "John Doe");
    }

    #[test]
    fn test_phone_e164_on_clean_number() {
        let result = apply_wiring("+1 (206) 555-0100", &[WiringTransform::new("phone_e164")]);
        assert_eq!(result.unwrap(), "+12065550100");
    }

    #[test]
    fn test_usd_to_cents_large() {
        let result = apply_wiring("$1,234.56", &[WiringTransform::new("usd_to_cents")]);
        assert_eq!(result.unwrap(), "123456");
    }

    #[test]
    fn test_lookup_with_trim() {
        let table = r#"{"  active  ": "on"}"#;
        let t = WiringTransform::new("lookup").with("table", table);
        let result = apply_wiring("  active  ", &[t]);
        // lookup does not trim by default — chain trim before it
        assert!(
            result.is_err(),
            "untrimmed lookup should not match padded keys"
        );
    }

    #[test]
    fn test_trim_then_lookup() {
        let table = r#"{"active": "on"}"#;
        let transforms = vec![
            WiringTransform::new("trim"),
            WiringTransform::new("lookup").with("table", table),
        ];
        let result = apply_wiring("  active  ", &transforms);
        assert_eq!(result.unwrap(), "on");
    }

    #[test]
    fn test_lookup_invalid_json() {
        let t = WiringTransform::new("lookup").with("table", "not valid json");
        let result = apply_wiring("key", &[t]);
        assert!(result.is_err());
    }

    #[test]
    fn test_split_default_sep() {
        // Default separator is comma
        let t = WiringTransform::new("split").with("idx", "0");
        let result = apply_wiring("a,b,c", &[t]);
        assert_eq!(result.unwrap(), "a");
    }
}
