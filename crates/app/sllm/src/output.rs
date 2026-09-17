//! Rendering of command results — one data model, two renderers (human and
//! `--json`) — plus the exit-code policy.
//!
//! Every renderer takes the writer it prints to, so tests render into a buffer
//! instead of capturing a process's stdout.
//!
//! Exit codes: `0` success, `2` error (a clap usage error exits `2` too).

use std::io::{self, Write};

use sllm_core::Greeting;

/// Exit code: success.
pub const EXIT_OK: u8 = 0;
/// Exit code: something failed.
pub const EXIT_FAILURE: u8 = 2;

/// Render a greeting, as a line of text or as a JSON object, and return the
/// exit code the process should use.
pub fn render_hello<W: Write>(w: &mut W, greeting: &Greeting, json: bool) -> io::Result<u8> {
    if json {
        // Serialized into a buffer first: `to_writer` would leave a partial
        // object behind if it failed mid-write, and callers pipe this into
        // `jq`.
        let buf = serde_json::to_vec(greeting)?;
        w.write_all(&buf)?;
        w.write_all(b"\n")?;
    } else {
        writeln!(w, "{}", greeting.message)?;
    }
    Ok(EXIT_OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sllm_core::greet;

    fn render(json: bool) -> (String, u8) {
        let mut buf = Vec::new();
        let code = render_hello(&mut buf, &greet(Some("ada")).unwrap(), json).unwrap();
        (String::from_utf8(buf).unwrap(), code)
    }

    #[test]
    fn human_output_is_one_line() {
        let (out, code) = render(false);
        insta::assert_snapshot!(out);
        assert_eq!(code, EXIT_OK);
    }

    #[test]
    fn json_output_is_one_object_per_line() {
        let (out, code) = render(true);
        assert_eq!(code, EXIT_OK);
        assert!(out.ends_with('\n'), "JSON output is newline-terminated");
        let doc: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(doc["name"], "ada");
        assert_eq!(doc["message"], "Hello, ada!");
    }

    #[test]
    fn a_closed_writer_surfaces_as_an_error_rather_than_a_panic() {
        // Stands in for a closed pipe: main classifies the error, so the
        // renderer must return it rather than unwrap internally.
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let greeting = greet(None).unwrap();
        for json in [false, true] {
            let e = render_hello(&mut Closed, &greeting, json).unwrap_err();
            assert_eq!(e.kind(), io::ErrorKind::BrokenPipe);
        }
    }
}
