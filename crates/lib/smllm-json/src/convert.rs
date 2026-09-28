//! [`JsonValue`] → typed values, with serde's rules for struct fields.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::{Error, JsonValue, parse};

/// A value that reads itself from a parsed tree.
pub trait FromJson: Sized {
    /// Read `Self` from `value`, taking it apart.
    fn from_json(value: JsonValue) -> Result<Self, Error>;
}

/// Parse `text` and read a `T` from it.
pub fn from_str<T: FromJson>(text: &str) -> Result<T, Error> {
    T::from_json(parse(text)?)
}

/// `result`, its error one field further out.
fn in_field<T>(result: Result<T, Error>, key: &str) -> Result<T, Error> {
    result.map_err(|e| e.in_field(key))
}

fn expected(what: &str, found: &JsonValue) -> Error {
    Error::new(format!("expected {what}, found {}", found.kind()))
}

/// An object's fields, taken out one by one by name, with serde's derive
/// rules: a required field must be there, a default field may be missing
/// (`Option` fields are default fields), a field given twice is an error,
/// and fields nobody takes are ignored.
pub struct Fields(Vec<(String, JsonValue)>);

impl FromJson for Fields {
    fn from_json(value: JsonValue) -> Result<Self, Error> {
        match value {
            JsonValue::Object(fields) => Ok(Fields(fields)),
            other => Err(expected("an object", &other)),
        }
    }
}

impl Fields {
    /// The field `key`, if present.
    pub fn take(&mut self, key: &str) -> Result<Option<JsonValue>, Error> {
        let Some(i) = self.0.iter().position(|(k, _)| k == key) else {
            return Ok(None);
        };
        let (_, value) = self.0.remove(i);
        if self.0[i..].iter().any(|(k, _)| k == key) {
            return Err(Error::new(format!("duplicate field `{key}`")));
        }
        Ok(Some(value))
    }

    /// The field `key`, which must be present.
    pub fn req<T: FromJson>(&mut self, key: &str) -> Result<T, Error> {
        let v = self.present(key)?;
        in_field(T::from_json(v), key)
    }

    /// The field `key`, or `T::default()` when it is missing.
    pub fn or_default<T: FromJson + Default>(&mut self, key: &str) -> Result<T, Error> {
        match self.take(key)? {
            Some(v) => in_field(T::from_json(v), key),
            None => Ok(T::default()),
        }
    }

    /// The generic-free half of [`Fields::req`], shared by every type.
    #[inline(never)]
    fn present(&mut self, key: &str) -> Result<JsonValue, Error> {
        self.take(key)?
            .ok_or_else(|| Error::new(format!("missing field `{key}`")))
    }

    /// The one field of an externally tagged enum (`{"variant": value}`).
    pub fn into_single(self) -> Result<(String, JsonValue), Error> {
        let mut fields = self.0;
        match fields.pop() {
            Some(field) if fields.is_empty() => Ok(field),
            _ => Err(Error::new("expected an object with one field")),
        }
    }

    /// The fields in document order (duplicates included), for maps.
    pub fn into_vec(self) -> Vec<(String, JsonValue)> {
        self.0
    }
}

impl FromJson for String {
    fn from_json(value: JsonValue) -> Result<Self, Error> {
        match value {
            JsonValue::String(s) => Ok(s),
            other => Err(expected("a string", &other)),
        }
    }
}

impl FromJson for bool {
    fn from_json(value: JsonValue) -> Result<Self, Error> {
        match value {
            JsonValue::Bool(b) => Ok(b),
            other => Err(expected("a boolean", &other)),
        }
    }
}

impl FromJson for i64 {
    fn from_json(value: JsonValue) -> Result<Self, Error> {
        match value {
            JsonValue::Int(n) => Ok(n),
            JsonValue::UInt(_) => Err(Error::new("integer out of range")),
            other => Err(expected("an integer", &other)),
        }
    }
}

impl FromJson for u64 {
    fn from_json(value: JsonValue) -> Result<Self, Error> {
        match value {
            JsonValue::Int(n) => u64::try_from(n).map_err(|_| Error::new("integer out of range")),
            JsonValue::UInt(n) => Ok(n),
            other => Err(expected("an integer", &other)),
        }
    }
}

impl FromJson for u32 {
    fn from_json(value: JsonValue) -> Result<Self, Error> {
        u32::try_from(u64::from_json(value)?).map_err(|_| Error::new("integer out of range"))
    }
}

impl<T: FromJson> FromJson for Option<T> {
    fn from_json(value: JsonValue) -> Result<Self, Error> {
        match value {
            JsonValue::Null => Ok(None),
            v => T::from_json(v).map(Some),
        }
    }
}

impl<T: FromJson> FromJson for Vec<T> {
    fn from_json(value: JsonValue) -> Result<Self, Error> {
        match value {
            JsonValue::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for (i, v) in items.into_iter().enumerate() {
                    match T::from_json(v) {
                        Ok(v) => out.push(v),
                        Err(e) => return Err(e.in_item(i)),
                    }
                }
                Ok(out)
            }
            other => Err(expected("an array", &other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    #[derive(Debug, PartialEq, Default)]
    struct Point {
        x: u32,
        label: Option<String>,
        tags: Vec<String>,
    }

    impl FromJson for Point {
        fn from_json(value: JsonValue) -> Result<Self, Error> {
            let mut f = Fields::from_json(value)?;
            Ok(Point {
                x: f.req("x")?,
                label: f.or_default("label")?,
                tags: f.or_default("tags")?,
            })
        }
    }

    #[test]
    fn fields_follow_serde_rules() {
        assert_eq!(
            from_str::<Point>(r#"{"x":1,"extra":[1.5,{}]}"#),
            Ok(Point {
                x: 1,
                ..Point::default()
            })
        );
        assert_eq!(
            from_str::<Point>(r#"{"x":1,"label":null,"tags":["a"]}"#),
            Ok(Point {
                x: 1,
                label: None,
                tags: vec!["a".into()]
            })
        );
        let err = |t: &str| from_str::<Point>(t).unwrap_err().to_string();
        assert_eq!(err(r#"{"label":"a"}"#), "missing field `x`");
        assert_eq!(err(r#"{"x":1,"x":2}"#), "duplicate field `x`");
        assert_eq!(err(r#"{"x":-1}"#), "x: integer out of range");
        assert_eq!(err(r#"{"x":4294967296}"#), "x: integer out of range");
        assert_eq!(
            err(r#"{"x":1.0}"#),
            "x: expected an integer, found a number"
        );
        assert_eq!(
            err(r#"{"x":1,"tags":null}"#),
            "tags: expected an array, found null"
        );
        assert_eq!(
            err(r#"{"x":1,"tags":["a",2]}"#),
            "tags[1]: expected a string, found an integer"
        );
        assert_eq!(err("[]"), "expected an object, found an array");
    }

    #[test]
    fn integers() {
        assert_eq!(from_str::<u64>("18446744073709551615"), Ok(u64::MAX));
        assert_eq!(from_str::<i64>("-9223372036854775808"), Ok(i64::MIN));
        assert!(from_str::<i64>("9223372036854775808").is_err());
        assert!(from_str::<u64>("-1").is_err());
        assert!(from_str::<u64>("-0").is_err());
        assert!(from_str::<bool>("1").is_err());
    }

    #[test]
    fn single_field_objects() {
        let one = |t: &str| {
            Fields::from_json(crate::parse(t).unwrap())
                .unwrap()
                .into_single()
        };
        assert_eq!(one(r#"{"a":1}"#), Ok(("a".into(), JsonValue::Int(1))));
        assert!(one("{}").is_err());
        assert!(one(r#"{"a":1,"b":2}"#).is_err());
    }
}
