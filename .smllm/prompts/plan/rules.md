Work under AGENTS.md and `.zen/rules/`. Build and test one thing at a time with `CARGO_BUILD_JOBS=4`
and no parallel build agents: parallel builds crash this container. Add no dependency without the
user's approval. When commit signing hangs, commit with `git -c commit.gpgsign=false commit`.
Explain things to the user in plain English.
