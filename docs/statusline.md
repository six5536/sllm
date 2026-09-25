# smllm in your status line

`smllm statusline` prints where the current session is (state machine, state and instance) as a
single row for your harness's status line:

```
smllm dev › WORK (visit 3) · issue GH-123
smllm dev › REVIEW · issue GH-123 · yielded
smllm idle · 1 suspended · 2 parked
```

It reads Claude Code's status JSON on stdin and finds the smllm session bound to its `session_id`.
When there is nothing to show (no smllm config, or a session smllm has not bound yet), it prints
nothing. It only reads, takes a few milliseconds, and always exits 0, so it can never blank the rest
of your status line.

## Set it up in Claude Code

Easiest: ask Claude, "add smllm to my status line". The `smllm-statusline` skill does it. It is
installed by `smllm harness install claude` (drop it with `--without statusline`) and shipped in
the plugin. The skill shows you the change before it makes it.

By hand: Claude Code runs one `statusLine` command, and each line it prints is a row. Add this
line at the end of your status line script, after its other output:

```sh
if row=$(printf '%s' "$input" | smllm statusline 2>/dev/null) && [ -n "$row" ]; then printf '\n%s' "$row"; fi
```

`$input` is the status JSON your script read from stdin (`input=$(cat)`). The `if` form
matters: Claude Code blanks the whole status line when the script exits non-zero, and a
trailing `[ -n "$row" ] && printf …` would do exactly that whenever there is nothing to show. With no script yet,
create `~/.claude/statusline-command.sh`:

```sh
#!/usr/bin/env bash
input=$(cat)
printf '%s' "$input" | smllm statusline
```

Then point Claude Code at it in `~/.claude/settings.json`:

```json
{ "statusLine": { "type": "command", "command": "bash ~/.claude/statusline-command.sh" } }
```

`smllm harness install` never changes `statusLine` itself: the setting is yours, and a
project-level one would override your collaborators' own status lines. `smllm harness status
claude` reminds you when your status line does not call `smllm statusline`.

## The default row

| Where        | Row                                                              |
| ------------ | ---------------------------------------------------------------- |
| In a machine | `smllm <machine> › <STATE>[ (visit n)] · <kind> <label>[ · yielded]` |
| In idle      | `smllm idle[ · n suspended][ · n parked]`                        |

- `(visit n)` appears from the second visit to a state.
- `<kind> <label>` is the instance, e.g. `issue GH-123`. The label is the instance's ref once
  set, else its id.
- `yielded` means the agent fired `yield`: it may stop until your next prompt.

Colours: `smllm` dim, the machine cyan, the state bold, the instance magenta, notes dim, and
`yielded` yellow. Colour is on by default, because a status line command never runs in a terminal
it could check. Turn it off with `NO_COLOR=1` or `--color never`.

## Options

```
smllm statusline [--session KEY] [--json] [--color auto|always|never]
```

- `--session KEY`: show this session instead of reading stdin.
- `--json`: print the fields below instead of the row.
- `--color`: `always`, `never`, or `auto` (always, unless `NO_COLOR` is set).

## The JSON fields

`smllm statusline --json` prints one object, or `{}` when there is nothing to show. Every key is
always present, with `null` when it does not apply.

| Field       | Type             | Meaning                                                |
| ----------- | ---------------- | ------------------------------------------------------ |
| `session`   | string           | The smllm session key (`sm-k7f3q2`)                    |
| `idle`      | bool             | True when no instance is held                          |
| `machine`   | string \| null   | The held instance's state machine                      |
| `state`     | string \| null   | Its current state                                      |
| `visit`     | number \| null   | Entries of that state so far                           |
| `yielded`   | bool             | The agent fired `yield` since your last prompt         |
| `instance`  | object \| null   | The held instance                                      |
| `suspended` | object \| null   | The instance put aside by `unmatched`                  |
| `parked`    | number           | Parked instances of the configured state machines     |

`instance` and `suspended` are `{machine, kind, id, ref, label, status}`: `kind` is what the
machine calls an instance (`meta.instance.kind`), `ref` is `null` until set, `label` is the ref
or else the id, and `status` is `active`, `suspended`, `parked` or `completed`.

New fields may be added. Before 1.0, a field is renamed or removed only in a minor release.

```json
{
  "session": "sm-k7f3q2",
  "idle": false,
  "machine": "dev",
  "state": "WORK",
  "visit": 3,
  "yielded": false,
  "instance": { "machine": "dev", "kind": "issue", "id": "a1b2c3", "ref": "GH-123", "label": "GH-123", "status": "active" },
  "suspended": null,
  "parked": 2
}
```

## Custom rows

Build your own row from `--json` with `jq` and `printf`, in place of the one-line default.

Only the state, in bold yellow:

```sh
s=$(printf '%s' "$input" | smllm statusline --json)
state=$(jq -r '.state // empty' <<<"$s")
if [ -n "$state" ]; then printf '\n\033[1;33m%s\033[0m' "$state"; fi
```

State and instance on the same row as your other fields, with nothing when idle:

```sh
s=$(printf '%s' "$input" | smllm statusline --json)
where=$(jq -r 'select(.state) | "\(.state) \(.instance.label)"' <<<"$s")
printf '%s%s' "$my_other_fields" "${where:+ | $where}"
```

A warning colour while the agent has yielded:

```sh
s=$(printf '%s' "$input" | smllm statusline --json)
if [ "$(jq -r '.yielded' <<<"$s")" = true ]; then
  printf '\n\033[43;30m waiting for you \033[0m'
fi
```

## Other harnesses

The command needs only a session key: `smllm statusline --session KEY` works from any
status bar, prompt or tmux line that knows the key. Without `--session`, stdin is read as
JSON with a `session_id` bound by the Claude Code hooks. Hosts using `smllm-wasm` get
the same fields from `engine.status(key)`.
