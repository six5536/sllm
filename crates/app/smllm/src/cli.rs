//! Command-line interface (clap derive), consistent with sokf's (CLI).
// @zen-component: CLI-Commands

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// State machines for LLM agents.
#[derive(Debug, Parser)]
// `version` feeds the man page header and `--help`; the built-in `-V` flag stays
// disabled because we render the version ourselves (see `main::run`).
#[command(name = "smllm", about, long_about = None, version, disable_version_flag = true)]
#[command(
    after_help = "Exit codes: 0 success, 1 errors found (config errors, a rejected event, a failed hook), 2 usage or internal error.\n\
    Docs: https://github.com/six5536/smllm"
)]
pub struct Cli {
    /// The command to run.
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Use only this config file (also SMLLM_CONFIG).
    #[arg(long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Print the version and exit.
    #[arg(short = 'V', long = "version", global = true)]
    pub version: bool,
}

/// Top-level commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create .smllm/config.toml here, or the user config.
    Init(InitArgs),
    /// Scaffold a state machine file (`<ID>.smllm.yaml`).
    New(NewArgs),
    /// Check the config, its state machines and saved instances.
    Validate(ValidateArgs),
    /// Fire an event: the smllm tool, for shell-only harnesses.
    Fire(FireArgs),
    /// Sessions: list, or show where one is.
    #[command(subcommand)]
    Session(SessionCommand),
    /// Instances: list, or show one with its history.
    #[command(subcommand)]
    Instance(InstanceCommand),
    /// Harness integration (Claude Code).
    #[command(subcommand)]
    Harness(HarnessCommand),
    /// States and transitions of the state machines.
    Graph(GraphArgs),
    /// Reference information.
    #[command(subcommand)]
    Info(InfoCommand),
    /// Emit the validated model as compact JSON, for smllm-wasm hosts.
    Compile(CompileArgs),
    /// Run the stdio MCP server with the one `smllm` tool.
    #[command(hide = true)]
    Mcp,
    /// Print a shell completion script to stdout.
    Completions(CompletionsArgs),
    /// Print a roff man page to stdout. Hidden: it exists for packaging, not
    /// day-to-day use, and is generated into the release archives.
    #[command(hide = true)]
    Man,
}

/// `init`.
#[derive(Debug, Args)]
pub struct InitArgs {
    /// Create the user config (~/.config/smllm/config.toml) instead.
    #[arg(long)]
    pub user: bool,
    /// Emit JSON.
    #[arg(long)]
    pub json: bool,
}

/// `new`.
#[derive(Debug, Args)]
pub struct NewArgs {
    /// The state machine id.
    #[arg(value_name = "ID")]
    pub id: String,
    /// Where to write it (default: the project's .smllm/).
    #[arg(long, value_name = "DIR")]
    pub dir: Option<PathBuf>,
    /// Write the file and register it in config.toml, instead of printing it.
    #[arg(long)]
    pub write: bool,
    /// Emit JSON.
    #[arg(long)]
    pub json: bool,
}

/// `validate`.
#[derive(Debug, Args)]
pub struct ValidateArgs {
    /// Config (.toml) or state machine (.yaml) files; default the configs found.
    #[arg(value_name = "PATHS")]
    pub paths: Vec<PathBuf>,
    /// Emit JSON.
    #[arg(long)]
    pub json: bool,
    /// List warnings.
    #[arg(long)]
    pub warnings: bool,
    /// List info findings.
    #[arg(long)]
    pub info: bool,
}

/// `fire`.
#[derive(Debug, Args)]
pub struct FireArgs {
    /// The session key; omit only with `enter`, to start a session.
    #[arg(long, value_name = "KEY")]
    pub session: Option<String>,
    /// The event.
    #[arg(value_name = "EVENT")]
    pub event: String,
    /// A param, repeatable.
    #[arg(long = "param", value_name = "KEY=VALUE")]
    pub params: Vec<String>,
    /// Emit JSON.
    #[arg(long)]
    pub json: bool,
}

/// `session …`.
#[derive(Debug, Subcommand)]
pub enum SessionCommand {
    /// List sessions, newest first.
    List(JsonArgs),
    /// Show where a session is: the tool's no-event view.
    Show(KeyArgs),
}

/// `instance …`.
#[derive(Debug, Subcommand)]
pub enum InstanceCommand {
    /// List instances of the configured state machines.
    List(JsonArgs),
    /// Show an instance (by id or ref) with its history.
    Show(KeyArgs),
}

/// Just `--json`.
#[derive(Debug, Args)]
pub struct JsonArgs {
    /// Emit JSON.
    #[arg(long)]
    pub json: bool,
}

/// A key or id plus `--json`.
#[derive(Debug, Args)]
pub struct KeyArgs {
    /// The session key, or instance id or ref.
    #[arg(value_name = "KEY")]
    pub key: String,
    /// Emit JSON.
    #[arg(long)]
    pub json: bool,
}

/// `harness …`.
#[derive(Debug, Subcommand)]
pub enum HarnessCommand {
    /// Install or update a harness integration.
    Install(HarnessArgs),
    /// Show the state of each part of a harness integration.
    Status(HarnessArgs),
    /// Hook entry point, called by the harness.
    #[command(hide = true)]
    Hook(HookArgs),
}

/// `harness install|status`.
#[derive(Debug, Args)]
pub struct HarnessArgs {
    /// The harness: claude.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// project or user.
    #[arg(long, default_value = "project", value_name = "S")]
    pub scope: String,
    /// Leave a part out (instructions, mcp, hooks, permissions); repeatable.
    #[arg(long, value_name = "PART")]
    pub without: Vec<String>,
    /// Overwrite parts edited by hand.
    #[arg(long)]
    pub force: bool,
    /// Emit JSON.
    #[arg(long)]
    pub json: bool,
}

/// `harness hook`.
#[derive(Debug, Args)]
pub struct HookArgs {
    /// The harness: claude.
    #[arg(value_name = "NAME")]
    pub name: String,
    /// session-start, user-prompt-submit or stop.
    #[arg(value_name = "HOOK")]
    pub hook: String,
}

/// `graph`.
#[derive(Debug, Args)]
pub struct GraphArgs {
    /// A state machine id; default all.
    #[arg(value_name = "ID")]
    pub id: Option<String>,
    /// Emit JSON.
    #[arg(long, conflicts_with = "mermaid")]
    pub json: bool,
    /// Emit a Mermaid stateDiagram-v2.
    #[arg(long)]
    pub mermaid: bool,
}

/// `info …`.
#[derive(Debug, Subcommand)]
pub enum InfoCommand {
    /// The JSON Schema of the state machine format.
    Schema,
}

/// `compile`.
#[derive(Debug, Args)]
pub struct CompileArgs {
    /// A config.toml or a state machine file.
    #[arg(value_name = "FILE")]
    pub file: PathBuf,
    /// Write here instead of stdout.
    #[arg(short = 'o', long = "out", value_name = "OUT")]
    pub out: Option<PathBuf>,
}

/// `completions`.
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
    fn globals_before_or_after_the_command() {
        assert!(parse(&["smllm", "validate", "-V"]).version);
        let c = parse(&["smllm", "validate", "--config", "x.toml"]);
        assert_eq!(c.config.as_deref(), Some(std::path::Path::new("x.toml")));
        let c = parse(&["smllm", "--config", "x.toml", "validate"]);
        assert!(c.config.is_some());
    }

    #[test]
    fn fire_takes_params_and_an_optional_session() {
        let Some(Command::Fire(f)) = parse(&[
            "smllm",
            "fire",
            "--session",
            "sm-1",
            "reject",
            "--param",
            "reason=x",
            "--param",
            "a=b",
        ])
        .command
        else {
            panic!()
        };
        assert_eq!(f.session.as_deref(), Some("sm-1"));
        assert_eq!(f.params, vec!["reason=x", "a=b"]);
    }

    #[test]
    fn graph_json_and_mermaid_conflict() {
        assert!(Cli::try_parse_from(["smllm", "graph", "--json", "--mermaid"]).is_err());
        assert!(Cli::try_parse_from(["smllm", "frobnicate"]).is_err());
    }
}
