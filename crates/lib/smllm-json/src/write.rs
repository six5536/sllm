//! Values → JSON text, byte-identical to serde_json's compact output.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

/// A value that writes itself as JSON.
pub trait ToJson {
    /// Append `self` as JSON to `out`.
    fn write_json(&self, out: &mut String);
}

/// `value` as JSON text.
pub fn to_string<T: ToJson + ?Sized>(value: &T) -> String {
    let mut out = String::new();
    value.write_json(&mut out);
    out
}

/// A JSON string, escaped as serde_json does: `"` and `\`, the short forms
/// `\b \t \n \f \r`, other control characters as lowercase `\u00xx`;
/// everything else (DEL and non-ASCII included) as is.
pub fn write_str(out: &mut String, s: &str) {
    out.push('"');
    let mut run = 0;
    for (i, b) in s.bytes().enumerate() {
        let short = match b {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            0x08 => "\\b",
            b'\t' => "\\t",
            b'\n' => "\\n",
            0x0c => "\\f",
            b'\r' => "\\r",
            0..=0x1f => "",
            _ => continue,
        };
        out.push_str(&s[run..i]);
        run = i + 1;
        if short.is_empty() {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            out.push_str("\\u00");
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0xf)]));
        } else {
            out.push_str(short);
        }
    }
    out.push_str(&s[run..]);
    out.push('"');
}

/// Writes a JSON object field by field.
pub struct ObjectWriter<'a> {
    out: &'a mut String,
    first: bool,
}

/// Start an object in `out`; add fields with [`ObjectWriter::field`], then
/// [`ObjectWriter::end`] it.
pub fn object(out: &mut String) -> ObjectWriter<'_> {
    out.push('{');
    ObjectWriter { out, first: true }
}

impl ObjectWriter<'_> {
    /// Add the field `key`.
    pub fn field(&mut self, key: &str, value: &dyn ToJson) -> &mut Self {
        if !self.first {
            self.out.push(',');
        }
        self.first = false;
        write_str(self.out, key);
        self.out.push(':');
        value.write_json(self.out);
        self
    }

    /// Close the object.
    pub fn end(self) {
        self.out.push('}');
    }
}

impl ToJson for str {
    fn write_json(&self, out: &mut String) {
        write_str(out, self);
    }
}

impl ToJson for String {
    fn write_json(&self, out: &mut String) {
        write_str(out, self);
    }
}

impl ToJson for bool {
    fn write_json(&self, out: &mut String) {
        out.push_str(if *self { "true" } else { "false" });
    }
}

macro_rules! integer {
    ($($t:ty),*) => {$(
        impl ToJson for $t {
            fn write_json(&self, out: &mut String) {
                let _ = write!(out, "{self}");
            }
        }
    )*};
}
integer!(u32, u64, i64);

impl<T: ToJson + ?Sized> ToJson for &T {
    fn write_json(&self, out: &mut String) {
        (**self).write_json(out);
    }
}

impl<T: ToJson> ToJson for Option<T> {
    fn write_json(&self, out: &mut String) {
        match self {
            Some(v) => v.write_json(out),
            None => out.push_str("null"),
        }
    }
}

impl<T: ToJson> ToJson for [T] {
    fn write_json(&self, out: &mut String) {
        out.push('[');
        for (i, v) in self.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            v.write_json(out);
        }
        out.push(']');
    }
}

impl<T: ToJson> ToJson for Vec<T> {
    fn write_json(&self, out: &mut String) {
        self.as_slice().write_json(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn strings_escape_as_serde_json_does() {
        assert_eq!(
            to_string("a\"b\\c\u{8}\t\n\u{c}\r\u{1}\u{1f}\u{7f}/é😀"),
            "\"a\\\"b\\\\c\\b\\t\\n\\f\\r\\u0001\\u001f\u{7f}/é😀\""
        );
        assert_eq!(to_string(""), "\"\"");
    }

    #[test]
    fn values_and_objects() {
        let mut out = String::new();
        let mut o = object(&mut out);
        o.field("n", &u64::MAX)
            .field("i", &i64::MIN)
            .field("b", &true)
            .field("none", &None::<u32>)
            .field("list", &vec![1u32, 2]);
        o.end();
        assert_eq!(
            out,
            r#"{"n":18446744073709551615,"i":-9223372036854775808,"b":true,"none":null,"list":[1,2]}"#
        );
        let mut empty = String::new();
        object(&mut empty).end();
        assert_eq!(empty, "{}");
        assert_eq!(to_string(&Vec::<u32>::new()), "[]");
    }
}
