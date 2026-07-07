//! reference/tokenkiller/canon.rs — canonical serializer (SPEC-009 TK1).
//! One byte of drift anywhere in S0–S2 invalidates every DeepSeek cache block
//! after that point. This module makes "same value ⇒ same bytes" a law:
//! sorted keys, NFC, LF, no insignificant whitespace, shortest-roundtrip floats.
//! Deps: serde_json, ryu, unicode-normalization.

use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

/// Canonical bytes for a JSON value. Idempotent: canon(parse(canon(x))) == canon(x).
pub fn to_bytes(v: &Value) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    write_value(v, &mut out);
    out
}

pub fn to_string(v: &Value) -> String {
    // Safety: write_value emits only valid UTF-8.
    String::from_utf8(to_bytes(v)).expect("canon emits UTF-8")
}

fn write_value(v: &Value, out: &mut Vec<u8>) {
    match v {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(true) => out.extend_from_slice(b"true"),
        Value::Bool(false) => out.extend_from_slice(b"false"),
        Value::Number(n) => write_number(n, out),
        Value::String(s) => write_string(s, out),
        Value::Array(a) => {
            out.push(b'[');
            for (i, item) in a.iter().enumerate() {
                if i > 0 { out.push(b','); }
                write_value(item, out);
            }
            out.push(b']');
        }
        Value::Object(m) => {
            // THE core rule: lexicographic key order by canonical (NFC) key bytes.
            let mut keys: Vec<(String, &Value)> =
                m.iter().map(|(k, val)| (nfc(k), val)).collect();
            keys.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            out.push(b'{');
            for (i, (k, val)) in keys.iter().enumerate() {
                if i > 0 { out.push(b','); }
                write_string(k, out);
                out.push(b':');
                write_value(val, out);
            }
            out.push(b'}');
        }
    }
}

fn write_number(n: &serde_json::Number, out: &mut Vec<u8>) {
    if let Some(i) = n.as_i64() {
        out.extend_from_slice(itoa(i).as_bytes());
    } else if let Some(u) = n.as_u64() {
        out.extend_from_slice(u.to_string().as_bytes());
    } else if let Some(f) = n.as_f64() {
        // Shortest round-trip formatting; NEVER format floats any other way in S0–S2.
        // NaN/inf are unrepresentable in JSON; serde_json already rejects them.
        let mut buf = ryu::Buffer::new();
        let s = buf.format_finite(f);
        // ryu prints integral floats as "1.0" — keep that (stability > prettiness).
        out.extend_from_slice(s.as_bytes());
    } else {
        // Arbitrary-precision feature off ⇒ unreachable; keep loud if enabled.
        out.extend_from_slice(n.to_string().as_bytes());
    }
}

fn itoa(i: i64) -> String { i.to_string() }

/// JSON string escaping, minimal escape set, on NFC-normalized text, LF-only.
fn write_string(s: &str, out: &mut Vec<u8>) {
    out.push(b'"');
    for c in nfc(s).replace("\r\n", "\n").replace('\r', "\n").chars() {
        match c {
            '"' => out.extend_from_slice(b"\\\""),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\n' => out.extend_from_slice(b"\\n"),
            '\t' => out.extend_from_slice(b"\\t"),
            c if (c as u32) < 0x20 => {
                out.extend_from_slice(format!("\\u{:04x}", c as u32).as_bytes());
            }
            c => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    out.push(b'"');
}

fn nfc(s: &str) -> String { s.nfc().collect() }

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tk_canon_sorts_keys_and_is_stable() {
        let a = json!({"b":1,"a":{"z":true,"y":[1.5,2]}});
        let b = json!({"a":{"y":[1.5,2],"z":true},"b":1});
        assert_eq!(to_bytes(&a), to_bytes(&b));
        assert_eq!(to_string(&a), r#"{"a":{"y":[1.5,2],"z":true},"b":1}"#);
    }

    #[test]
    fn tk_canon_idempotent() {
        let v = json!({"k":"caf\u{0065}\u{0301}","n":[0.1,10,-3]}); // e + combining acute
        let once = to_bytes(&v);
        let reparsed: Value = serde_json::from_slice(&once).unwrap();
        assert_eq!(once, to_bytes(&reparsed)); // NFC applied ⇒ fixed point
    }

    // In crates/tokenkiller add the proptest version:
    // proptest! { fn tk_prop_canon_idempotent(v in arb_json()) { ... } }
}
