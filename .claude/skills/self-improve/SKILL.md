---
name: self-improve
description: >
  Analyze past Claude Code session transcripts to find recurring failures
  and turn them into concrete CLAUDE.md rules. Trigger when the user says
  "self-improve", "improve from sessions", "/self-improve", or points at a
  traces folder with intent to improve future sessions.
---

# self-improve

Read past Claude Code session transcripts, identify recurring mistakes, and
propose concrete rules to write into `CLAUDE.md` so future sessions don't
repeat them. Never apply changes without explicit human approval.

## Inputs

Ask the user for these if not provided:

1. **`TRACES`** — where the session transcripts are. Default to the current
   project's logs at `~/.claude/projects/<project-slug>/*.jsonl`. The user may
   instead point at a `.claude/traces/` folder they curated.
2. **`SIGNAL`** _(optional)_ — how to tell a good session from a bad one. If
   the user names one (e.g. "tests passing", "clean build", "sessions I tagged
   bad"), use it. Otherwise infer it using the Stage 2 heuristics. State which
   signal you used.
3. **`SCOPE`** _(optional)_ — how many / which sessions. Default to the most
   recent 10–20 sessions.

## Outputs

- `.claude/eval/findings.md` — recurring failure patterns, with evidence.
- `.claude/eval/proposed-rules.md` — candidate `CLAUDE.md` rules, for review.
- `.claude/eval/learning-log.md` — append-only history of what was applied and when.
- Approved rules written into a managed block in `CLAUDE.md`.

Create the `.claude/eval/` directory if it doesn't exist.

---

## Stage 1 — Gather

Locate the trace files under `TRACES`. Report how many sessions you found and
the date range. Each `.jsonl` line is one event (user message, assistant
message, tool call, tool result). Process sessions in small batches using sub-agents and write intermediate notes to `.claude/eval/findings.md` rather than holding everything in context at once.

## Stage 2 — Determine the signal

For each session, classify it as **success**, **failure**, or **mixed**, and
record why. If `SIGNAL` was given, use it. Otherwise infer from the trace:

Failure indicators:

- User corrections: "no", "that's wrong", "don't", "undo", "revert", "actually
  I meant", or the same request rephrased multiple times.
- The assistant making several attempts at one task, or looping.
- Errors in tool results: failed tests, build/compile errors, stack traces,
  linter failures, command exit errors.
- The user manually re-editing a file right after the assistant edited it.
- Long back-and-forth on something that should have been one step.

Success indicators:

- Tests/build passing in tool results.
- User confirmation ("works", "that's it", "thanks") then moving on.
- Task completed in few turns with no corrections.

If you can't find a usable signal, say so plainly and stop — do not invent
rules from sessions you can't judge.

## Stage 3 — Cluster failures

Across the failing/mixed sessions, group the mistakes into recurring patterns:

- A convention the assistant kept violating (naming, file layout, imports).
- A wrong default it kept reaching for (a framework, a command, an approach).
- Missing project context it had to rediscover each time.
- A class of bug it repeatedly introduced.

A pattern needs at least two independent occurrences, ideally across different
sessions; ignore one-offs. Write each pattern to `.claude/eval/findings.md` with a short
name, its occurrence count, and 1–2 brief trace excerpts as evidence (session id

- a few words).

## Stage 4 — Draft fixes as rules

For each recurring pattern, write one concrete, imperative rule suitable for
`CLAUDE.md` (e.g. "Use named exports, never default exports" — not "improve code
style"). Keep rules strict and concise. Each entry in `.claude/eval/proposed-rules.md` carries the rule text, the
pattern it addresses with its count, and a confidence note.

Before proposing, check `.claude/eval/learning-log.md` and the existing managed block in
`CLAUDE.md` — don't re-propose an applied rule. If a pattern recurs despite a
rule already existing, flag that rule for revision instead of duplicating it.

If a rule only matters for part of the codebase, propose a path-scoped rule
under `.claude/rules/` instead of adding it to the global `CLAUDE.md`.

## Stage 5 — Human review gate

Present the proposed rules with their evidence. For each, let the user
**approve**, **edit**, or **reject**. Do not proceed until you have an explicit
decision for each rule.

## Stage 6 — Apply and log

Write approved rules into `CLAUDE.md` inside a managed block, replacing the
previous block's contents so re-runs update rather than duplicate:

```
<!-- BEGIN self-improve (managed) -->
## Learned rules
- <approved rule 1>
- <approved rule 2>
<!-- END self-improve (managed) -->
```

Keep the block concise; if the list outgrows ~200 total lines in `CLAUDE.md`,
merge or retire weaker rules rather than appending forever.

Append a dated entry to `.claude/eval/learning-log.md`: date, sessions analyzed, signal
used, and each rule applied/edited/rejected with the pattern it came from.

Report to the user: sessions analyzed, signal used, patterns found, rules
applied.
