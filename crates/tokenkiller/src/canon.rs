use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

pub fn to_bytes(value: &Value) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    write_value(value, &mut out);
    out
}

pub fn to_string(value: &Value) -> String {
    String::from_utf8(to_bytes(value)).expect("canon emits valid UTF-8")
}

fn write_value(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(true) => out.extend_from_slice(b"true"),
        Value::Bool(false) => out.extend_from_slice(b"false"),
        Value::Number(number) => write_number(number, out),
        Value::String(text) => write_string(text, out),
        Value::Array(items) => {
            out.push(b'[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                write_value(item, out);
            }
            out.push(b']');
        }
        Value::Object(map) => {
            let mut keys = map
                .iter()
                .map(|(key, value)| (nfc(key), value))
                .collect::<Vec<_>>();
            keys.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));

            out.push(b'{');
            for (index, (key, value)) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                write_string(key, out);
                out.push(b':');
                write_value(value, out);
            }
            out.push(b'}');
        }
    }
}

fn write_number(number: &serde_json::Number, out: &mut Vec<u8>) {
    if let Some(value) = number.as_i64() {
        out.extend_from_slice(value.to_string().as_bytes());
    } else if let Some(value) = number.as_u64() {
        out.extend_from_slice(value.to_string().as_bytes());
    } else if let Some(value) = number.as_f64() {
        let mut buffer = ryu::Buffer::new();
        out.extend_from_slice(buffer.format_finite(value).as_bytes());
    } else {
        out.extend_from_slice(number.to_string().as_bytes());
    }
}

fn write_string(text: &str, out: &mut Vec<u8>) {
    out.push(b'"');
    for ch in nfc(text).replace("\r\n", "\n").replace('\r', "\n").chars() {
        match ch {
            '"' => out.extend_from_slice(b"\\\""),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\n' => out.extend_from_slice(b"\\n"),
            '\t' => out.extend_from_slice(b"\\t"),
            ch if (ch as u32) < 0x20 => {
                out.extend_from_slice(format!("\\u{:04x}", ch as u32).as_bytes());
            }
            ch => {
                let mut buffer = [0_u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    out.push(b'"');
}

fn nfc(text: &str) -> String {
    text.nfc().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::{json, Number, Value};

    fn arb_string() -> impl Strategy<Value = String> {
        prop::collection::vec(any::<char>(), 0..16)
            .prop_map(|chars| chars.into_iter().collect::<String>())
    }

    fn arb_json() -> impl Strategy<Value = Value> {
        let leaf = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(|value| Value::Number(value.into())),
            any::<u64>().prop_map(|value| Value::Number(value.into())),
            (any::<i32>(), 0_u16..1000_u16, any::<bool>()).prop_map(|(whole, frac, negative)| {
                let mut value = f64::from(whole) + f64::from(frac) / 1000.0;
                if negative {
                    value = -value;
                }
                Value::Number(Number::from_f64(value).expect("finite float"))
            }),
            arb_string().prop_map(Value::String),
        ];

        leaf.prop_recursive(4, 64, 8, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
                prop::collection::btree_map(arb_string(), inner, 0..4)
                    .prop_map(|entries| Value::Object(entries.into_iter().collect())),
            ]
        })
    }

    #[test]
    fn tk_canon_sorts_keys_and_is_stable() {
        let left = json!({"b":1,"a":{"z":true,"y":[1.5,2]}});
        let right = json!({"a":{"y":[1.5,2],"z":true},"b":1});
        assert_eq!(to_bytes(&left), to_bytes(&right));
        assert_eq!(to_string(&left), r#"{"a":{"y":[1.5,2],"z":true},"b":1}"#);
    }

    #[test]
    fn tk_canon_normalizes_unicode_and_line_endings() {
        let value = json!({"k":"caf\u{0065}\u{0301}\r\nline"});
        assert_eq!(to_string(&value), r#"{"k":"café\nline"}"#);
    }

    proptest! {
        #[test]
        fn tk_prop_canon_idempotent(value in arb_json()) {
            let once = to_bytes(&value);
            let reparsed: Value =
                serde_json::from_slice(&once).expect("canonical bytes must reparse");
            prop_assert_eq!(once, to_bytes(&reparsed));
        }
    }
}
