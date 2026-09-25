# smllm Claude Code plugin

The one-command install of smllm for Claude Code (PLAN-001 D34): the three
hooks (`SessionStart`, `UserPromptSubmit`, `Stop`) and the `smllm` MCP server.
It is the same wiring `smllm harness install claude` writes into a project,
minus the `AGENTS.md`/`CLAUDE.md` block (the MCP tool description carries the
rules).

```sh
npm install -g smllm                       # the binary the plugin calls
claude plugin marketplace add six5536/smllm
claude plugin install smllm@smllm
```

Then add a `.smllm/config.toml` to a project (`smllm init`).
