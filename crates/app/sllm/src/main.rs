//! sllm CLI entry point.
// Under the nightly coverage job (cargo-llvm-cov sets `coverage_nightly`), enable
// the attribute used to exclude genuinely untestable glue from coverage. Inert on
// the stable toolchain used for normal builds and tests.
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![warn(missing_docs)]

mod cli;
mod man;
mod output;

use std::io;
use std::path::Path;
use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use sllm_core::{Error, greet};

use crate::cli::{Cli, Command, CompletionsArgs, HelloArgs};

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        // A downstream reader went away (`| head`, a pager quit early). That is
        // not our failure, so say nothing and exit clean, the same as ripgrep and
        // friends. Rust sets SIGPIPE to SIG_IGN, so this reaches us as an EPIPE
        // write error instead of killing the process.
        Err(e) if is_broken_pipe(&e) => ExitCode::from(output::EXIT_OK),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(output::EXIT_FAILURE)
        }
    }
}

/// Whether an error is just a closed downstream pipe rather than a real fault.
fn is_broken_pipe(e: &Error) -> bool {
    matches!(e, Error::Io { source, .. } if source.kind() == io::ErrorKind::BrokenPipe)
}

fn run() -> sllm_core::Result<u8> {
    let cli = Cli::parse();
    if cli.version {
        // `name x.y.z`, the near-universal CLI convention, so pasted output is
        // self-identifying in bug reports.
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(output::EXIT_OK);
    }
    let Some(command) = cli.command else {
        Cli::command().print_help().ok();
        return Ok(output::EXIT_OK);
    };
    match command {
        Command::Hello(args) => run_hello(args),
        Command::Completions(args) => run_completions(args),
        Command::Man => run_man(),
    }
}

/// Greet someone. The skeleton's one real command: parse → core → render.
fn run_hello(args: HelloArgs) -> sllm_core::Result<u8> {
    let greeting = greet(args.name.as_deref())?;
    output::render_hello(&mut io::stdout().lock(), &greeting, args.json).map_err(stdout_err)
}

/// Write a shell completion script for `shell` to stdout.
fn run_completions(args: CompletionsArgs) -> sllm_core::Result<u8> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    // Rendered into a buffer rather than straight to stdout: `generate` returns
    // `()` and `expect()`s its writes internally, so handing it a closed pipe
    // aborts the process (`panic = "abort"`) instead of surfacing an error we
    // can classify. Writing the finished buffer ourselves keeps that in our hands.
    let mut buf = Vec::new();
    clap_complete::generate(args.shell, &mut cmd, name, &mut buf);
    write_stdout(&buf)?;
    Ok(output::EXIT_OK)
}

/// Write a roff man page to stdout, for packaging into release archives.
fn run_man() -> sllm_core::Result<u8> {
    let buf = man::render().map_err(stdout_err)?;
    write_stdout(&buf)?;
    Ok(output::EXIT_OK)
}

/// Write a fully-rendered buffer to stdout and flush it. The explicit flush
/// matters: the runtime's flush at exit discards its error, which made a broken
/// pipe surface only when the buffer happened to fill mid-render.
fn write_stdout(bytes: &[u8]) -> sllm_core::Result<()> {
    use io::Write as _;
    let mut out = io::stdout().lock();
    out.write_all(bytes).map_err(stdout_err)?;
    out.flush().map_err(stdout_err)
}

/// Map a stdout write failure into a `sllm` error so render helpers can use `?`.
fn stdout_err(e: io::Error) -> Error {
    Error::io(Path::new("<stdout>"), e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_pipe_is_the_only_error_treated_as_clean() {
        let epipe = Error::io(
            Path::new("<stdout>"),
            io::Error::new(io::ErrorKind::BrokenPipe, "Broken pipe"),
        );
        assert!(is_broken_pipe(&epipe));

        // Any other I/O failure is a real one, however similar it looks.
        for kind in [
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::NotFound,
            io::ErrorKind::WriteZero,
        ] {
            let e = Error::io(Path::new("<stdout>"), io::Error::new(kind, "nope"));
            assert!(!is_broken_pipe(&e), "{kind:?} must not be swallowed");
        }
        // Nor is a non-I/O error, which has no `kind` to inspect at all.
        assert!(!is_broken_pipe(&Error::config("blank name")));
    }

    #[test]
    fn stdout_err_names_the_stream_it_failed_on() {
        let e = stdout_err(io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe"));
        assert!(e.to_string().contains("<stdout>"), "{e}");
        assert!(is_broken_pipe(&e));
    }

    #[test]
    fn write_stdout_round_trips_bytes() {
        // Captured by the test harness; the point is that it reports success
        // rather than panicking, and handles an empty buffer.
        assert!(write_stdout(b"sllm\n").is_ok());
        assert!(write_stdout(b"").is_ok());
    }

    #[test]
    fn hello_renders_through_the_core() {
        let args = HelloArgs {
            name: Some("ada".into()),
            json: false,
        };
        assert_eq!(run_hello(args).unwrap(), output::EXIT_OK);

        // A blank name is the core's error, surfaced unchanged.
        let blank = HelloArgs {
            name: Some("  ".into()),
            json: true,
        };
        assert!(matches!(run_hello(blank), Err(Error::Config(_))));
    }
}
