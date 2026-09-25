// Derived from sokf 9c93f37 crates/lib/sokf-core/src/harness/state.rs
//! The 64-bit FNV-1a hash the record and the loop guard use.

use std::fmt::Write as _;

/// The FNV-1a hash of a text, line endings normalised to LF, as `fnv1a64:`
/// and sixteen lowercase hex digits.
pub fn hash_text(text: &str) -> String {
    let mut h = Fnv::new();
    h.update(normalise(text).as_bytes());
    h.render()
}

/// CRLF to LF.
pub fn normalise(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// An incremental FNV-1a hasher.
pub(crate) struct Fnv(u64);

impl Fnv {
    pub(crate) fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    pub(crate) fn render(&self) -> String {
        let mut s = String::from("fnv1a64:");
        let _ = write!(s, "{:016x}", self.0);
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors_and_line_endings() {
        assert_eq!(hash_text(""), "fnv1a64:cbf29ce484222325");
        assert_eq!(hash_text("a"), "fnv1a64:af63dc4c8601ec8c");
        assert_eq!(hash_text("foobar"), "fnv1a64:85944171f73967e8");
        assert_eq!(hash_text("x\r\ny\r\n"), hash_text("x\ny\n"));
    }
}
