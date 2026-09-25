---
name: smllm-statusline
description: Use when the user wants smllm's state (state machine, state, instance) in their Claude Code status line, or asks to set up, change the look of, or remove the smllm status line row.
---

# smllm in the Claude Code status line

`smllm statusline` reads Claude Code's status JSON on stdin and prints one row for the session it
is bound to, e.g. `smllm dev › WORK (visit 3) · issue GH-123`, or `smllm idle · 2 parked`. It
prints nothing when there is nothing to show and always exits 0. `smllm statusline --json` prints
the same fields as JSON for custom rows. Reference: https://github.com/six5536/smllm/blob/main/docs/statusline.md

Claude Code has exactly one `statusLine` command; each line it prints is one row. Add smllm as its
own row to the user's command. Never replace or reorder the user's existing rows.

## Rules

- Show every change as a diff and apply it only after the user agrees.
- Edit only the status line script and the `statusLine` setting; keep the rest of the settings file
  as it is.
- Read stdin once in the script (`input=$(cat)`) and pass `"$input"` to every command that needs it.
- The script must exit 0: Claude Code blanks the whole status line on a non-zero exit. Guard
  optional rows with `if …; then …; fi`, never with a trailing `[ … ] && printf …` (false when
  there is nothing to show, which becomes the script's exit status). Keep `[ -n "$row" ]` so
  that nothing to show adds no empty row.

## Add the row

1. Find the `statusLine` setting: `.claude/settings.local.json`, then `.claude/settings.json`, then
   `~/.claude/settings.json` (the first one that has it is used). Check `command -v smllm`; if smllm
   is missing, say so and stop.
2. No `statusLine` anywhere: create `~/.claude/statusline-command.sh`:

   ```sh
   #!/usr/bin/env bash
   input=$(cat)
   printf '%s' "$input" | smllm statusline
   ```

   Make it executable, and set in `~/.claude/settings.json`:
   `"statusLine": { "type": "command", "command": "bash ~/.claude/statusline-command.sh" }`.
3. The command runs a script (e.g. `bash ~/.claude/statusline-command.sh`): make sure the script
   has `input=$(cat)` (add it near the top if it reads stdin another way, and use `"$input"` there),
   then append at the end:

   ```sh
   if row=$(printf '%s' "$input" | smllm statusline 2>/dev/null) && [ -n "$row" ]; then printf '\n%s' "$row"; fi
   ```

   If the script's last output already ends with a newline, use `printf '%s' "$row"` instead of
   `printf '\n%s'`, so there is no blank row.
4. The command is inline (not a script): move it into `~/.claude/statusline-command.sh` unchanged
   after `input=$(cat)`, feeding it `"$input"` on stdin, append the line from step 3, and point
   `statusLine.command` at the script.

## Change the look

The default row colours: `smllm` dim, machine cyan, state bold, instance magenta, notes dim,
`yielded` yellow. `NO_COLOR=1` or `--color never` turns colour off. For anything else, replace the
smllm line with a block over `--json`, for example state in bold yellow and no instance:

```sh
s=$(printf '%s' "$input" | smllm statusline --json)
state=$(jq -r '.state // empty' <<<"$s")
if [ -n "$state" ]; then printf '\n\033[1;33m%s\033[0m' "$state"; fi
```

Fields: `session`, `idle`, `machine`, `state`, `visit`, `yielded`, `parked`, and `instance` /
`suspended` (`null` or `{machine, kind, id, ref, label, status}`); `{}` when there is nothing to
show. Use `jq` only if it is installed (`command -v jq`).

## Verify

Run the status line command with a sample status JSON, show the user the rows, and check that it
exits 0:

```sh
echo '{"session_id":"test","cwd":"'"$PWD"'","model":{"display_name":"Opus"}}' | bash ~/.claude/statusline-command.sh; echo " [exit $?]"
```

An unbound `session_id` gives no smllm row; that is expected. To see a row, use this session's id:
check `smllm session list` for a session bound in this project, or run
`smllm statusline --session <KEY>`. The row appears in Claude Code after the next assistant message.

## Remove

Delete the smllm line (or block) from the script. If the script only printed the smllm row and was
created by this skill, offer to remove the script and the `statusLine` setting too.
