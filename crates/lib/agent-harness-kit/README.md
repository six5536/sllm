# agent-harness-kit

Shared plumbing for command-line tools that plug into LLM agent harnesses
(Claude Code first): installing and reporting the files a harness reads,
answering its hooks, reporting findings, and the usual CLI output
conventions.

The code is derived from [sokf](https://github.com/six5536/sokf) at commit
`9c93f37` and generalised so that any tool can supply its own content. Each
file taken from sokf starts with a `// Derived from sokf 9c93f37 <path>`
line. The crate is `std`, has no dependency on smllm or sokf, and embeds no
content of its own.

## Modules

| Module    | What it gives you                                                                                                                                                                                                                                                                                                                                                                                             |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `harness` | `Tool` (your tool's name, a `Profile` per harness and `Scope`, the root directory, the record file, the declined-parts store) and `install` / `status` over it. A profile is a list of `Part`s of four write kinds: `file` (the tool owns whole files), `region` (a block between `<!-- <tool>:harness -->` markers in `AGENTS.md` / `CLAUDE.md`, compared by words), `merge` (entries in a JSON file, set by `MergeOp`s, keeping key order and indent), and `external` (a part the tool reads and writes itself, such as `claude mcp add-json`). Part states: skipped, absent, current, stale, edited. |
| `hook`    | Claude Code's hook input (`HookInput`) and answers (`Answer`: `{}`, a `Stop` block, or `hookSpecificOutput.additionalContext`), `emit` (answer on stdout, or exit 1 with the error on stderr), and `LoopGuard` (block only once on the same text, per key).                                                                                                                                                                                         |
| `report`  | `Finding` (error, warning or info, with an authority), `Report`, and `report_text`: `<path>:<line>: <level>: <message> (<authority>)` lines and a counts line.                                                                                                                                                                                                                                                  |
| `cli`     | Exit codes (0 ok, 1 errors found, 2 usage or internal error), `write_stdout`, `json_line`, `is_broken_pipe`, and `finish` / `exit_code`, which print `error: <message>` on stderr.                                                                                                                                                                                                                              |
