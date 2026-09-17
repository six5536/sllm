//! Command-line interface (clap derive).

use clap::{Args, Parser, Subcommand};

/// A skeleton Rust CLI, shipped by a complete release pipeline.
#[derive(Debug, Parser)]
// `version` feeds the man page header and `--help`; the built-in `-V` flag stays
// disabled because we render the version ourselves (see `main::run`).
#[command(name = "sllm", about, long_about = None, version, disable_version_flag = true)]
#[command(after_help = "Exit codes: 0 success, 2 error.\n\
    Docs: https://github.com/six5536/sllm")]
pub struct Cli {
    /// The command to run.
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Print the version and exit.
    #[arg(short = 'V', long = "version", global = true)]
    pub version: bool,
}

/// Top-level commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print a greeting — the placeholder verb this skeleton ships with.
    Hello(HelloArgs),
    /// Print a shell completion script to stdout.
    Completions(CompletionsArgs),
    /// Print a roff man page to stdout. Hidden: it exists for packaging, not
    /// day-to-day use, and is generated into the release archives.
    #[command(hide = true)]
    Man,
}

/// Arguments for `hello`.
#[derive(Debug, Args)]
pub struct HelloArgs {
    /// Who to greet. Defaults to the world.
    #[arg(value_name = "NAME")]
    pub name: Option<String>,

    /// Emit machine-readable JSON instead of human output.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `completions`.
#[derive(Debug, Args)]
pub struct CompletionsArgs {
    /// Shell to generate a completion script for.
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).unwrap()
    }

    #[test]
    fn bare_invocation_has_no_command() {
        let cli = parse(&["sllm"]);
        assert!(cli.command.is_none());
        assert!(!cli.version);
    }

    #[test]
    fn version_flag_has_both_spellings() {
        assert!(parse(&["sllm", "-V"]).version);
        assert!(parse(&["sllm", "--version"]).version);
        // Global, so it is accepted after a subcommand too.
        assert!(parse(&["sllm", "hello", "--version"]).version);
    }

    #[test]
    fn hello_takes_an_optional_name_and_json_flag() {
        let Some(Command::Hello(args)) = parse(&["sllm", "hello"]).command else {
            panic!("expected hello");
        };
        assert_eq!(args.name, None);
        assert!(!args.json);

        let Some(Command::Hello(args)) = parse(&["sllm", "hello", "ada", "--json"]).command else {
            panic!("expected hello");
        };
        assert_eq!(args.name.as_deref(), Some("ada"));
        assert!(args.json);
    }

    #[test]
    fn completions_requires_a_known_shell() {
        let Some(Command::Completions(args)) = parse(&["sllm", "completions", "bash"]).command
        else {
            panic!("expected completions");
        };
        assert_eq!(args.shell, clap_complete::Shell::Bash);
        assert!(Cli::try_parse_from(["sllm", "completions", "cmd.exe"]).is_err());
        assert!(Cli::try_parse_from(["sllm", "completions"]).is_err());
    }

    #[test]
    fn man_is_parsed_though_hidden_from_help() {
        assert!(matches!(
            parse(&["sllm", "man"]).command,
            Some(Command::Man)
        ));
    }

    #[test]
    fn unknown_commands_and_flags_are_rejected() {
        assert!(Cli::try_parse_from(["sllm", "frobnicate"]).is_err());
        assert!(Cli::try_parse_from(["sllm", "--definitely-not-a-flag"]).is_err());
    }
}
