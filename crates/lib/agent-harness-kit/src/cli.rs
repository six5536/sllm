// Derived from sokf 9c93f37 crates/app/sokf/src/output.rs and crates/app/sokf/src/main.rs
//! CLI conventions: the exit codes, buffered stdout, broken-pipe detection
//! and the `error: <message>` runner.

use std::{
    error::Error as StdError,
    io::{self, Write},
    process::ExitCode,
};

use serde::Serialize;

/// Exit code: no error.
pub const EXIT_OK: u8 = 0;
/// Exit code: errors found (e.g. config errors, a rejected event, a failed
/// hook).
pub const EXIT_ERRORS: u8 = 1;
/// Exit code: a usage or internal error, reported on stderr.
pub const EXIT_FAILURE: u8 = 2;

/// Write a fully rendered buffer to stdout and flush it. The explicit flush
/// matters: the runtime's flush at exit discards its error, so a broken
/// pipe would surface only when the buffer happened to fill mid-render.
pub fn write_stdout(bytes: &[u8]) -> io::Result<()> {
    write_all(&mut io::stdout().lock(), bytes)
}

fn write_all(out: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    out.write_all(bytes)?;
    out.flush()
}

/// `value` as one JSON object and a newline, nothing else.
pub fn json_line(value: &impl Serialize) -> serde_json::Result<Vec<u8>> {
    let mut buf = serde_json::to_vec(value)?;
    buf.push(b'\n');
    Ok(buf)
}

/// Whether an error is, or is caused by, a closed downstream pipe rather
/// than a real fault.
pub fn is_broken_pipe(error: &(dyn StdError + 'static)) -> bool {
    let mut cur = Some(error);
    while let Some(e) = cur {
        if e.downcast_ref::<io::Error>()
            .is_some_and(|io| io.kind() == io::ErrorKind::BrokenPipe)
        {
            return true;
        }
        cur = e.source();
    }
    false
}

/// The exit code of a command's result, writing `error: <message>` to
/// `stderr` for a failure. A broken pipe is a clean exit, as in ripgrep
/// and friends: the reader went away (`| head`), which is not our failure.
pub fn exit_code<E: StdError + 'static>(result: Result<u8, E>, stderr: &mut impl Write) -> u8 {
    match result {
        Ok(code) => code,
        Err(e) if is_broken_pipe(&e) => EXIT_OK,
        Err(e) => {
            let _ = writeln!(stderr, "error: {e}");
            EXIT_FAILURE
        }
    }
}

/// The process exit of a command's result: see [`exit_code`]; the message
/// goes to the process's stderr.
pub fn finish<E: StdError + 'static>(result: Result<u8, E>) -> ExitCode {
    ExitCode::from(exit_code(result, &mut io::stderr()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn broken_pipe_is_the_only_error_treated_as_clean() {
        let epipe = Error::io(
            "<stdout>",
            io::Error::new(io::ErrorKind::BrokenPipe, "Broken pipe"),
        );
        assert!(is_broken_pipe(&epipe));
        assert!(is_broken_pipe(&io::Error::from(io::ErrorKind::BrokenPipe)));
        for kind in [
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::NotFound,
            io::ErrorKind::WriteZero,
        ] {
            let e = Error::io("<stdout>", io::Error::new(kind, "nope"));
            assert!(!is_broken_pipe(&e), "{kind:?} must not be swallowed");
        }
        assert!(!is_broken_pipe(&Error::Harness("no".into())));
    }

    #[test]
    fn exit_codes_and_the_error_line() {
        let mut err = Vec::new();
        assert_eq!(exit_code::<Error>(Ok(EXIT_ERRORS), &mut err), 1);
        let pipe = Error::io("<stdout>", io::Error::from(io::ErrorKind::BrokenPipe));
        assert_eq!(exit_code(Err(pipe), &mut err), 0);
        assert!(err.is_empty());
        assert_eq!(
            exit_code(Err(Error::Harness("no profile".into())), &mut err),
            2
        );
        assert_eq!(String::from_utf8(err).unwrap(), "error: no profile\n");
        assert_eq!(finish::<Error>(Ok(0)), ExitCode::from(0));
    }

    #[test]
    fn stdout_helpers() {
        assert!(write_stdout(b"").is_ok());
        let mut buf = Vec::new();
        write_all(&mut buf, b"x\n").unwrap();
        assert_eq!(buf, b"x\n");
        assert_eq!(
            json_line(&serde_json::json!({"a": 1})).unwrap(),
            b"{\"a\":1}\n"
        );
    }
}
