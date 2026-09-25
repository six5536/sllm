Implement the next phase of the plan that is not done yet (check `git log` and the plan).
Write the specs first when the phase is a spec phase; keep the `@zen-*` markers and the `.zen/specs/`
documents in step with the code. Snapshot tests: review each new `.snap.new` before accepting it.

Commit the phase (`<type>: <what> (PLAN-nnn Sn)`), then fire phaseDone with the phase name: the
machine runs `npm run test:gate` (tests + clippy) and sends you on, or to FIX. After the last
phase, fire allPhasesDone instead.
