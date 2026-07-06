//! Tolerant JSON hydration helpers over [`serde_json::Value`].
//!
//! The model layer must never panic on a surprising payload: a field that is
//! missing, `null`, or an unexpected type degrades to a default rather than an
//! error. These helpers mirror the sibling SDKs' `Data`/`data.go` support layer.

use serde_json::{Map, Value};

/// A JSON string, or `None` if the value is absent / not a string. Numbers and
/// booleans are **not** stringified (a surprising type yields `None`).
pub(crate) fn opt_string(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    }
}

/// A JSON string, or `default` when absent / not a string.
pub(crate) fn string(v: Option<&Value>, default: &str) -> String {
    opt_string(v).unwrap_or_else(|| default.to_string())
}

/// A nullable integer. Accepts a JSON number (integers directly; floats are
/// truncated toward zero) or a numeric string. Rejects booleans, null, arrays
/// and objects. `i64` precision is preserved for large file sizes.
pub(crate) fn opt_i64(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Number(n)) => n
            .as_i64()
            .or_else(|| n.as_u64().map(|u| u as i64))
            .or_else(|| n.as_f64().map(|f| f.trunc() as i64)),
        Some(Value::String(s)) => {
            let t = s.trim();
            t.parse::<i64>()
                .ok()
                .or_else(|| t.parse::<f64>().ok().map(|f| f.trunc() as i64))
        }
        _ => None,
    }
}

/// A JSON object as a `Map`, or an empty map when absent / not an object.
pub(crate) fn object(v: Option<&Value>) -> Map<String, Value> {
    match v {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    }
}

/// A JSON array, passed through. An object is coerced to its values ordered by
/// key (deterministic); anything else yields an empty list.
pub(crate) fn list(v: Option<&Value>) -> Vec<Value> {
    match v {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::Object(m)) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            keys.into_iter().map(|k| m[k].clone()).collect()
        }
        _ => Vec::new(),
    }
}

/// Map a factory over a JSON array (or object-as-list), yielding a typed `Vec`.
pub(crate) fn map_objects<T>(v: Option<&Value>, factory: impl Fn(&Value) -> T) -> Vec<T> {
    list(v).iter().map(&factory).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn opt_i64_accepts_numbers_and_numeric_strings() {
        assert_eq!(opt_i64(Some(&json!(42))), Some(42));
        assert_eq!(opt_i64(Some(&json!("42"))), Some(42));
        assert_eq!(opt_i64(Some(&json!(3.9))), Some(3)); // truncate toward zero
                                                         // Precision beyond 2^53 is preserved.
        assert_eq!(
            opt_i64(Some(&json!(9_007_199_254_740_993i64))),
            Some(9_007_199_254_740_993)
        );
        assert_eq!(opt_i64(Some(&json!(true))), None); // reject bool
        assert_eq!(opt_i64(Some(&json!("nope"))), None);
        assert_eq!(opt_i64(None), None);
    }

    #[test]
    fn opt_string_does_not_stringify_non_strings() {
        assert_eq!(opt_string(Some(&json!("hi"))), Some("hi".to_string()));
        assert_eq!(opt_string(Some(&json!(5))), None);
        assert_eq!(opt_string(Some(&json!(null))), None);
        assert_eq!(opt_string(None), None);
    }

    #[test]
    fn list_coerces_object_to_sorted_values() {
        let v = json!({"b": 2, "a": 1});
        assert_eq!(list(Some(&v)), vec![json!(1), json!(2)]);
        assert_eq!(list(Some(&json!([3, 4]))), vec![json!(3), json!(4)]);
        assert!(list(Some(&json!("x"))).is_empty());
    }

    #[test]
    fn object_defaults_to_empty() {
        assert!(object(Some(&json!("nope"))).is_empty());
        assert_eq!(object(Some(&json!({"k": 1}))).get("k"), Some(&json!(1)));
    }
}
