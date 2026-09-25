# PLAN-002: smllm in the status line

| Meta               | Value                                                                                  |
| ------------------ | -------------------------------------------------------------------------------------- |
| Status             | in-progress (grilled and decided, §8; ready to implement from S0)                      |
| Workflow direction | top-down                                                                               |
| Traces to          | `REQ-STL-statusline.md`, `DESIGN-STL-statusline.md` (to be written); CLI, HOST, NFR    |

## 1. Goal

Show where smllm has the agent (state machine, state, instance) in Claude Code's status bar, on
its own row, with colour. Composable with an existing status line, flexible in what is shown and
how, set up by a skill, and usable by other harnesses.

Flexibility comes from data, not a template language (D2-1): smllm prints a sensible coloured
default row, or the fields as JSON for the user's own status script to shape with `jq`/`printf`,
the way Claude Code status lines are already written.

## 2. How Claude Code's status line works (docs, 2026-09)

- One `statusLine` setting: `{type: "command", command, padding?, refreshInterval?}`, in user,
  project, local or managed settings. There is exactly one command, so tools compose by that
  command calling others. Plugins cannot set it (only `subagentStatusLine`).
- stdin: JSON with `session_id`, `cwd`, `workspace.{current_dir,project_dir}`, `model`,
  `context_window`, … Several fields may be absent or null.
- stdout: each line is its own row; ANSI SGR colours and OSC 8 links work. Width comes from
  `COLUMNS` (no TTY). Non-zero exit blanks the whole status line.
- Re-run on session start/resume, each assistant message, `/compact`, permission-mode change,
  optional `refreshInterval`. Debounced 300 ms; a new run cancels an in-flight one.
- This machine: `~/.claude/settings.json` → `bash ~/.claude/statusline-command.sh` (dir, git,
  model, context bar on one row, `jq` + `printf` colours).

## 3. Shape of the solution

Default row, added under the user's existing one:

```sh
# ~/.claude/statusline-command.sh (end)
if row=$(printf '%s' "$input" | smllm statusline 2>/dev/null) && [ -n "$row" ]; then printf '\n%s' "$row"; fi
```

```
smllm dev › WORK (visit 3) · issue GH-123
smllm idle · 1 suspended · 2 parked
```

A custom row, from the fields:

```sh
s=$(printf '%s' "$input" | smllm statusline --json)
state=$(jq -r '.state // empty' <<<"$s")
if [ -n "$state" ]; then printf '\n\033[1;33m%s\033[0m %s' "$state" "$(jq -r '.instance.label' <<<"$s")"; fi
```

Nothing to show (no config, no binding) → the default row is empty and `--json` is `{}`, so the
caller adds no row. `smllm statusline` is read-only and never fails loudly.

## 4. Requirements (STL)

| ID     | Requirement                                                                                          |
| ------ | ---------------------------------------------------------------------------------------------------- |
| STL-1  | `smllm statusline [--session KEY] [--json] [--color auto\|always\|never]`                            |
| STL-2  | Session: `--session`, else the Claude Code status JSON on stdin → `session_id` → binding (HOST-4). Unknown/unbound/no config → empty row / `{}`, exit 0 |
| STL-3  | Read-only: no store writes, no guard/action commands, no binding; < 20 ms typical (NFR-2)           |
| STL-4  | Never blanks the host's status line: any failure → empty row / `{}`, exit 0; the reason on stderr  |
| STL-5  | `--json` (D2-6, D2-8): `{session, idle, machine, state, visit, yielded, instance, suspended, parked}`, `instance`/`suspended` each `{machine, kind, id, ref, label, status}`; `instance`/`suspended` `null` when none; one object (CLI conventions); a stable, documented contract |
| STL-6  | Default row (D2-7): in a machine `smllm <machine> › <STATE>[ (visit n)] · <kind> <label>[ · yielded]`; in idle `smllm idle[ · n suspended][ · n parked]`, parts only when non-zero; colours: `smllm` dim, machine cyan, state bold, instance magenta, notes dim, `yielded` yellow |
| STL-7  | Colour: `--color`, else `NO_COLOR` set → never, else always (the host captures output, so TTY detection would always say no) |
| STL-8  | Core exposes read-only `Engine::status(key) -> Status` (no_std; used by the app and `smllm-wasm`)   |
| STL-9  | Skill `smllm-statusline`: adds the smllm row to the user's existing status line (or creates a minimal one), customises it on request by writing `jq`/`printf` against `--json`, verifies with a sample JSON; shipped in the plugin and by `harness install` as the default part `statusline` (D2-10) |
| STL-10 | Doc `docs/statusline.md` (the repo's first `docs/` page; D2-11): quick setup (skill or one line), the default row, the JSON fields, custom-row examples, other harnesses; linked from README |
| STL-11 | `harness status claude` adds a note when the user's `statusLine` command (or the script it runs) does not call `smllm statusline`: "ask Claude to add smllm to your status line" |

Non-functional: STL-3/4 as NFR-2/4; snapshot the default rows and the JSON (TEST-1); no new
dependencies.

## 5. Design sketch

| Where                                    | What                                                                 |
| ---------------------------------------- | -------------------------------------------------------------------- |
| `smllm-core` `engine`                    | `Status { session, idle, machine, state, visit, yielded, instance: Option<InstanceStatus>, suspended: Option<InstanceStatus>, parked }` (serde, camelCase); `Engine::status` (reads only) |
| app `commands/statusline.rs`             | stdin JSON → key → `Engine::status` → default row (SGR) or JSON       |
| `smllm-wasm`                             | `Engine.status(key)` → JSON                                          |
| app skill template → `.claude/skills/smllm-statusline/SKILL.md` | Installed by the `statusline` harness part (a kit `files` part) |
| `plugin/skills/smllm-statusline/SKILL.md` | Same skill in the plugin                                            |
| `docs/statusline.md`                     | User doc                                                              |

## 6. The skill

`smllm-statusline` (description: "Use when the user wants smllm's state in their Claude Code
status line, or asks to set up, change or remove it"). Steps:

1. Read the `statusLine` setting (user → project → local).
2. None: create `~/.claude/statusline-command.sh` printing only the smllm row; set `statusLine`.
3. A script: append the one composition line (§3), keeping its rows as they are; a plain
   command: wrap it in a script that runs it, then adds the smllm row. Show the diff; apply on
   consent.
4. Customise on request ("state in bold yellow, hide the ref"): replace the line with a
   `jq`/`printf` block over `smllm statusline --json` (§3).
5. Verify: pipe a sample status JSON (with a bound `session_id` when one exists) through the
   command; show the rows.
6. Remove on request: take the lines out again.

## 7. Phases

| #  | Phase         | Output                                                                     | Traces     |
| -- | ------------- | -------------------------------------------------------------------------- | ---------- |
| S0 | Rename        | `noun` → `kind` (D2-9): format, core, app text, examples, schema, specs, tests | CFG-6      |
| S1 | Specs         | `REQ-STL-statusline.md`, `DESIGN-STL-statusline.md`; CLI/ARCHITECTURE updates | all     |
| S2 | Core          | `Engine::status`, tests; `smllm-wasm` `status`                              | STL-3, 8   |
| S3 | Command       | `smllm statusline`, default row, `--json`, colour rules, snapshots, e2e      | STL-1–7    |
| S4 | Skill + doc   | Skill (plugin + `statusline` harness part), status hint, `docs/statusline.md`, README | STL-9–11 |
| S5 | This machine  | Run the skill on `~/.claude/statusline-command.sh`; confirm the row live     | STL-9      |

## 8. Decisions

| #    | Decision                                                                                     |
| ---- | -------------------------------------------------------------------------------------------- |
| D2-1 | No template syntax: default row + `--json`; customisation lives in the user's script         |
| D2-2 | `harness install` never sets `statusLine` (personal, one per user; a project one would override collaborators'); it installs the skill, which edits the status line on request with consent (STL-11) |
| D2-3 | Nothing to show (no config, unbound, before SessionStart binds) → no row; a bound session in idle does show `smllm idle …` |
| D2-4 | Default row is plain text (`smllm`, `›`, `·`); no Nerd Font glyphs (custom rows can add them) |
| D2-5 | No `subagentStatusLine` row: only the main agent uses smllm (D15)                            |
| D2-6 | `--json` contract: camelCase keys, every key always present (`null` when not applicable) except `{}` for nothing-to-show; snapshot-tested; add freely, rename/remove only in a minor release pre-1.0 (NFR-6) |
| D2-7 | Default row content and colours as STL-6; no session key, events or guard results in the row |
| D2-8 | JSON groups instance fields under `instance` and `suspended`, one shape `{machine, kind, id, ref, label, status}` (same keys in both, D2-6) |
| D2-9 | Rename `noun` → `kind` everywhere (`meta.instance.kind`, `InstanceSpec.kind`, examples, schema, specs), before S1; pre-release, no migration |
| D2-10 | The skill is harness part `statusline`: default in both scopes (project `.claude/skills/`, user `~/.claude/skills/`), excluded with `--without statusline` (remembered); the plugin always ships it |
| D2-11 | User doc at `docs/statusline.md` (reference for the JSON contract and recipes); README and the skill link to it rather than repeating it |
