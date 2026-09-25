//! Man page rendering.
//!
//! `clap_mangen` renders only the top-level command, which left every verb's
//! flags undocumented. This module keeps its title/name/synopsis/description
//! and global-options sections, then appends hand-written roff: a COMMANDS
//! section with each verb's flags, EXIT STATUS, and FILES.

use std::fmt::Write as _;

use clap::CommandFactory;

use crate::cli::Cli;

/// Render the complete man page as roff.
pub fn render() -> std::io::Result<Vec<u8>> {
    let man = clap_mangen::Man::new(Cli::command());
    let mut buf = Vec::new();
    man.render_title(&mut buf)?;
    man.render_name_section(&mut buf)?;
    man.render_synopsis_section(&mut buf)?;
    man.render_description_section(&mut buf)?;
    man.render_options_section(&mut buf)?;

    let mut extra = String::new();
    render_commands(&mut extra);
    render_exit_status(&mut extra);
    render_files(&mut extra);
    buf.extend_from_slice(extra.as_bytes());

    man.render_version_section(&mut buf)?;
    Ok(buf)
}

/// One `.SS` subsection per visible verb, with its positionals and flags.
///
/// Iterates an *unbuilt* command: `clap::Command::build` propagates the global
/// flags and adds `--help` to every subcommand, which would repeat
/// `-V`/`--version` under each verb here.
fn render_commands(out: &mut String) {
    out.push_str(".SH COMMANDS\n");
    for sub in Cli::command().get_subcommands() {
        if sub.is_hide_set() {
            continue;
        }
        let _ = writeln!(out, ".SS \"{}\"", usage(sub));
        let about = sub.get_about().map(ToString::to_string).unwrap_or_default();
        let aliases: Vec<&str> = sub.get_visible_aliases().collect();
        if aliases.is_empty() {
            out.push_str(&text(&about));
        } else {
            out.push_str(&text(&format!("{about} (alias: {})", aliases.join(", "))));
        }
        for arg in sub.get_arguments() {
            if arg.get_id() == "help" || arg.is_global_set() {
                continue;
            }
            out.push_str(".TP\n");
            let _ = writeln!(out, "{}", arg_header(arg));
            let help = arg.get_help().map(ToString::to_string).unwrap_or_default();
            out.push_str(&text(&help));
        }
    }
}

/// The `.SS` heading: `smllm <verb> [<POSITIONAL>…] [OPTIONS]`.
fn usage(sub: &clap::Command) -> String {
    let mut s = format!("smllm {}", sub.get_name());
    for pos in sub.get_positionals() {
        let _ = write!(s, " <{}>", value_name(pos));
    }
    if sub.get_arguments().any(|a| !a.is_positional()) {
        s.push_str(" [OPTIONS]");
    }
    s
}

/// The bold flag spelling (or italic positional) that heads a `.TP` entry.
fn arg_header(arg: &clap::Arg) -> String {
    if arg.is_positional() {
        return format!("\\fI<{}>\\fR", value_name(arg));
    }
    let mut s = String::new();
    match (arg.get_short(), arg.get_long()) {
        (Some(short), Some(long)) => {
            let _ = write!(s, "\\fB\\-{short}\\fR, \\fB\\-\\-{}\\fR", opt(long));
        }
        (Some(short), None) => {
            let _ = write!(s, "\\fB\\-{short}\\fR");
        }
        (None, Some(long)) => {
            let _ = write!(s, "\\fB\\-\\-{}\\fR", opt(long));
        }
        (None, None) => {}
    }
    if arg.get_action().takes_values() {
        let _ = write!(s, " \\fI<{}>\\fR", value_name(arg));
    }
    s
}

/// An arg's `<VALUE>` placeholder: the declared `value_name`, or the id.
fn value_name(arg: &clap::Arg) -> String {
    arg.get_value_names()
        .and_then(|names| names.first())
        .map(ToString::to_string)
        .unwrap_or_else(|| arg.get_id().to_string().to_uppercase())
}

fn render_exit_status(out: &mut String) {
    out.push_str(
        ".SH EXIT STATUS\n\
         .TP\n.B 0\nSuccess.\n\
         .TP\n.B 1\nErrors found: config errors, a rejected event, a failed hook.\n\
         .TP\n.B 2\nA usage or internal error.\n",
    );
}

fn render_files(out: &mut String) {
    out.push_str(
        ".SH FILES\n\
         .TP\n.I .smllm/config.toml\nThe project config: state machine files, idle text.\n\
         .TP\n.I ~/.config/smllm/config.toml\nThe user config, combined with the project's.\n\
         .TP\n.I .smllm/state/\nInstances and their history (beside each config).\n\
         .TP\n.I ~/.local/state/smllm/\nSessions and harness bindings.\n",
    );
}

/// A roff text line: escape backslashes and protect a leading control character.
fn text(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\");
    if escaped.starts_with('.') || escaped.starts_with('\'') {
        format!("\\&{escaped}\n")
    } else {
        format!("{escaped}\n")
    }
}

/// Escape the dashes inside a long option name (`dry-run` → `dry\-run`).
fn opt(long: &str) -> String {
    long.replace('-', "\\-")
}
