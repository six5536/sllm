//! JSON text → [`JsonValue`]: recursive descent over `&str`, RFC 8259.
//!
//! It accepts exactly what RFC 8259 (and serde_json) accept: one value,
//! surrounding whitespace, no trailing content, no comments or trailing
//! commas, the strict number grammar, no raw control characters or lone
//! surrogates in strings. Containers nest at most [`MAX_DEPTH`] deep, so a
//! hostile document is an error, never a stack overflow.

use alloc::string::String;
use alloc::vec::Vec;

use crate::{Error, JsonValue};

/// The deepest nesting of objects and arrays accepted (serde_json's limit).
pub const MAX_DEPTH: usize = 128;

/// Parse a JSON document.
pub fn parse(input: &str) -> Result<JsonValue, Error> {
    let mut p = Parser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    p.skip_ws();
    let value = p.value(0)?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(p.fail("trailing characters after the JSON value"));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn fail(&self, message: &str) -> Error {
        Error::at_byte(message, self.pos)
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    /// One value; `depth` containers are already open around it.
    fn value(&mut self, depth: usize) -> Result<JsonValue, Error> {
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => Ok(JsonValue::String(self.string()?)),
            Some(b't') => self.literal("true", JsonValue::Bool(true)),
            Some(b'f') => self.literal("false", JsonValue::Bool(false)),
            Some(b'n') => self.literal("null", JsonValue::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.fail("unexpected character")),
            None => Err(self.fail("unexpected end of input")),
        }
    }

    fn open(&mut self, depth: usize) -> Result<(), Error> {
        if depth >= MAX_DEPTH {
            return Err(self.fail("nested too deeply"));
        }
        self.pos += 1;
        self.skip_ws();
        Ok(())
    }

    /// After an item: `true` at the closing `close`, `false` after a comma.
    fn next_or_close(&mut self, close: u8) -> Result<bool, Error> {
        self.skip_ws();
        match self.peek() {
            Some(b',') => {
                self.pos += 1;
                self.skip_ws();
                Ok(false)
            }
            Some(b) if b == close => {
                self.pos += 1;
                Ok(true)
            }
            _ => Err(self.fail(if close == b'}' {
                "expected `,` or `}`"
            } else {
                "expected `,` or `]`"
            })),
        }
    }

    fn object(&mut self, depth: usize) -> Result<JsonValue, Error> {
        self.open(depth)?;
        let mut fields = Vec::new();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(fields));
        }
        loop {
            if self.peek() != Some(b'"') {
                return Err(self.fail("expected a key string"));
            }
            let key = self.string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(self.fail("expected `:`"));
            }
            self.pos += 1;
            self.skip_ws();
            fields.push((key, self.value(depth + 1)?));
            if self.next_or_close(b'}')? {
                return Ok(JsonValue::Object(fields));
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<JsonValue, Error> {
        self.open(depth)?;
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Array(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            if self.next_or_close(b']')? {
                return Ok(JsonValue::Array(items));
            }
        }
    }

    fn literal(&mut self, word: &str, value: JsonValue) -> Result<JsonValue, Error> {
        if !self.bytes[self.pos..].starts_with(word.as_bytes()) {
            return Err(self.fail("invalid literal"));
        }
        self.pos += word.len();
        Ok(value)
    }

    fn string(&mut self) -> Result<String, Error> {
        self.pos += 1; // the opening quote
        let mut out = String::new();
        loop {
            // Copy the run up to the next quote, backslash or control
            // character in one go: the input is `&str`, and those are ASCII,
            // so the run ends on a char boundary.
            let start = self.pos;
            while matches!(self.peek(), Some(b) if b != b'"' && b != b'\\' && b >= 0x20) {
                self.pos += 1;
            }
            if let Ok(run) = core::str::from_utf8(&self.bytes[start..self.pos]) {
                out.push_str(run);
            }
            match self.peek() {
                None => return Err(self.fail("unterminated string")),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    let c = match self.peek() {
                        Some(b'"') => '"',
                        Some(b'\\') => '\\',
                        Some(b'/') => '/',
                        Some(b'b') => '\u{8}',
                        Some(b'f') => '\u{c}',
                        Some(b'n') => '\n',
                        Some(b'r') => '\r',
                        Some(b't') => '\t',
                        Some(b'u') => {
                            out.push(self.unicode_escape()?);
                            continue;
                        }
                        _ => return Err(self.fail("invalid escape")),
                    };
                    out.push(c);
                    self.pos += 1;
                }
                // RFC 8259 §7: control characters must be escaped.
                Some(_) => return Err(self.fail("control character in string")),
            }
        }
    }

    /// `\uXXXX`, or a UTF-16 surrogate pair of them; `pos` is on the `u`.
    fn unicode_escape(&mut self) -> Result<char, Error> {
        self.pos += 1;
        let hi = self.hex4()?;
        let code = if (0xD800..0xDC00).contains(&hi) {
            if !self.bytes[self.pos..].starts_with(b"\\u") {
                return Err(self.fail("lone surrogate in \\u escape"));
            }
            self.pos += 2;
            let lo = self.hex4()?;
            if !(0xDC00..0xE000).contains(&lo) {
                return Err(self.fail("lone surrogate in \\u escape"));
            }
            0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
        } else {
            hi
        };
        char::from_u32(code).ok_or_else(|| self.fail("lone surrogate in \\u escape"))
    }

    fn hex4(&mut self) -> Result<u32, Error> {
        let mut value = 0;
        for _ in 0..4 {
            let digit = match self.peek() {
                Some(b @ b'0'..=b'9') => b - b'0',
                Some(b @ b'a'..=b'f') => b - b'a' + 10,
                Some(b @ b'A'..=b'F') => b - b'A' + 10,
                _ => return Err(self.fail("invalid \\u escape")),
            };
            value = value * 16 + u32::from(digit);
            self.pos += 1;
        }
        Ok(value)
    }

    /// `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`: an integer
    /// in range becomes `Int` / `UInt`, anything else `Number` (its text).
    fn number(&mut self) -> Result<JsonValue, Error> {
        let start = self.pos;
        let negative = self.peek() == Some(b'-');
        if negative {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(Error::at_byte("invalid number", start)),
        }
        let mut integral = true;
        if self.peek() == Some(b'.') {
            self.pos += 1;
            self.expect_digits(start)?;
            integral = false;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            self.expect_digits(start)?;
            integral = false;
        }
        // ASCII only, so always valid UTF-8.
        let text = core::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or_default();
        if integral {
            // `-0` is not an integer to serde_json either (it reads a float).
            if let Ok(n) = text.parse::<i64>()
                && !(negative && n == 0)
            {
                return Ok(JsonValue::Int(n));
            }
            if let Ok(n) = text.parse::<u64>() {
                return Ok(JsonValue::UInt(n));
            }
        }
        Ok(JsonValue::Number(text.into()))
    }

    fn digits(&mut self) {
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
    }

    fn expect_digits(&mut self, start: usize) -> Result<(), Error> {
        let first = self.pos;
        self.digits();
        if self.pos == first {
            return Err(Error::at_byte("invalid number", start));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    fn s(v: &str) -> JsonValue {
        JsonValue::String(v.into())
    }

    #[test]
    fn scalars() {
        assert_eq!(parse("true"), Ok(JsonValue::Bool(true)));
        assert_eq!(parse("false"), Ok(JsonValue::Bool(false)));
        assert_eq!(parse("null"), Ok(JsonValue::Null));
        assert_eq!(parse("42"), Ok(JsonValue::Int(42)));
        assert_eq!(parse("-7"), Ok(JsonValue::Int(-7)));
        assert_eq!(parse(" \n 42 \n "), Ok(JsonValue::Int(42)));
    }

    #[test]
    fn numbers_beyond_integers_stay_text() {
        assert_eq!(parse("9223372036854775807"), Ok(JsonValue::Int(i64::MAX)));
        assert_eq!(parse("-9223372036854775808"), Ok(JsonValue::Int(i64::MIN)));
        assert_eq!(parse("18446744073709551615"), Ok(JsonValue::UInt(u64::MAX)));
        for n in ["18446744073709551616", "3.5", "1e21", "-0", "-2.5E-3"] {
            assert_eq!(parse(n), Ok(JsonValue::Number(n.into())), "{n}");
        }
    }

    #[test]
    fn rejects_non_rfc_numbers() {
        for n in [
            "01", "1.", "-.5", "1e", "1e+", "+1", "-", ".5", "00", "1.2.3", "0x10",
        ] {
            assert!(parse(n).is_err(), "took {n:?}");
        }
    }

    #[test]
    fn strings_and_escapes() {
        assert_eq!(parse(r#""a\n\t\"b\\\/""#), Ok(s("a\n\t\"b\\/")));
        assert_eq!(parse(r#""\b\f\r\u0001""#), Ok(s("\u{8}\u{c}\r\u{1}")));
        assert_eq!(parse(r#""Aé""#), Ok(s("Aé")));
        assert_eq!(parse(r#""😀 é""#), Ok(s("😀 é")));
        assert_eq!(parse("\"\u{7f}\""), Ok(s("\u{7f}")));
    }

    #[test]
    fn rejects_bad_strings() {
        for t in [
            "\"a\tb\"",
            "\"a\nb\"",
            r#""\q""#,
            r#""\uD800""#,
            r#""\uDC00""#,
            r#""\uD800A""#,
            r#""\u12""#,
            r#""abc"#,
        ] {
            assert!(parse(t).is_err(), "took {t:?}");
        }
    }

    #[test]
    fn containers_keep_order_and_duplicates() {
        assert_eq!(
            parse(r#"{"z":1,"a":[true,null],"z":{}}"#),
            Ok(JsonValue::Object(vec![
                ("z".into(), JsonValue::Int(1)),
                (
                    "a".into(),
                    JsonValue::Array(vec![JsonValue::Bool(true), JsonValue::Null])
                ),
                ("z".into(), JsonValue::Object(vec![])),
            ]))
        );
        assert_eq!(parse("[ ]"), Ok(JsonValue::Array(vec![])));
        assert_eq!(parse("{ }"), Ok(JsonValue::Object(vec![])));
    }

    #[test]
    fn rejects_malformed_documents() {
        for t in [
            "",
            "42 oops",
            "{}{}",
            "{",
            "[1,]",
            "{\"a\":1,}",
            "{\"a\" 1}",
            "{a:1}",
            "[1 2]",
            "// c\n1",
            "tru",
            "nul",
            "{\"machines\":[],\"zzz\":}",
            "{\"machines\":[],\"zzz\":[",
        ] {
            assert!(parse(t).is_err(), "took {t:?}");
        }
        assert_eq!(
            parse("[1, x]").unwrap_err().to_string(),
            "unexpected character at byte 4"
        );
    }

    #[test]
    fn nesting_is_bounded() {
        let nest = |n: usize| "[".repeat(n) + &"]".repeat(n);
        assert!(parse(&nest(MAX_DEPTH)).is_ok());
        assert_eq!(
            parse(&nest(MAX_DEPTH + 1)).unwrap_err().to_string(),
            "nested too deeply at byte 128"
        );
        assert!(parse(&nest(100_000)).is_err());
    }
}
