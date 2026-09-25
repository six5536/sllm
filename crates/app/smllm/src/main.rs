//! smllm CLI entry point.
// Under the nightly coverage job (cargo-llvm-cov sets `coverage_nightly`), enable
// the attribute used to exclude genuinely untestable glue from coverage. Inert on
// the stable toolchain used for normal builds and tests.
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![warn(missing_docs)]

mod cli;
mod commands;
mod error;
mod host;
mod man;
mod output;
mod paths;
mod runtime;
mod store;

use std::process::ExitCode;

use clap::{CommandFactory, Parser};

use crate::cli::{
    Cli, Command, CompletionsArgs, HarnessCommand, InfoCommand, InstanceCommand, SessionCommand,
};
use crate::commands::{config, graph, harness, mcp, state};
use crate::error::Result;

fn main() -> ExitCode {
    agent_harness_kit::cli::finish(run())
}

fn run() -> Result<u8> {
    let cli = Cli::parse();
    if cli.version {
        // `name x.y.z`, the near-universal CLI convention, so pasted output is
        // self-identifying in bug reports.
        output::text(&format!(
            "{} {}\n",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION")
        ))?;
        return Ok(output::EXIT_OK);
    }
    let Some(command) = cli.command else {
        Cli::command().print_help().ok();
        return Ok(output::EXIT_OK);
    };
    let explicit = cli.config.as_deref();
    match command {
        Command::Init(a) => config::init(&a),
        Command::New(a) => config::new(&a, explicit),
        Command::Validate(a) => config::validate(&a, explicit),
        Command::Fire(a) => state::fire(&a, explicit),
        Command::Session(SessionCommand::List(a)) => state::session_list(&a),
        Command::Session(SessionCommand::Show(a)) => state::session_show(&a),
        Command::Instance(InstanceCommand::List(a)) => state::instance_list(&a, explicit),
        Command::Instance(InstanceCommand::Show(a)) => state::instance_show(&a, explicit),
        Command::Harness(HarnessCommand::Install(a)) => harness::run(&a, true),
        Command::Harness(HarnessCommand::Status(a)) => harness::run(&a, false),
        Command::Harness(HarnessCommand::Hook(a)) => harness::hook(&a),
        Command::Graph(a) => graph::graph(&a, explicit),
        Command::Info(InfoCommand::Schema) => config::schema(),
        Command::Compile(a) => config::compile_cmd(&a),
        Command::Mcp => mcp::serve(explicit),
        Command::Completions(a) => completions(&a),
        Command::Man => man::render()
            .map_err(|e| error::Error::io(std::path::Path::new("<stdout>"), e))
            .and_then(|b| {
                agent_harness_kit::cli::write_stdout(&b)
                    .map_err(|e| error::Error::io(std::path::Path::new("<stdout>"), e))?;
                Ok(output::EXIT_OK)
            }),
    }
}

/// Write a shell completion script for `shell` to stdout.
fn completions(args: &CompletionsArgs) -> Result<u8> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    // Rendered into a buffer rather than straight to stdout: `generate`
    // `expect()`s its writes, so a closed pipe would abort the process
    // (`panic = "abort"`) instead of surfacing an error we can classify.
    let mut buf = Vec::new();
    clap_complete::generate(args.shell, &mut cmd, name, &mut buf);
    agent_harness_kit::cli::write_stdout(&buf)
        .map_err(|e| error::Error::io(std::path::Path::new("<stdout>"), e))?;
    Ok(output::EXIT_OK)
}
