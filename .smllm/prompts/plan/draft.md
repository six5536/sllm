Write the plan. Read `.zen/specs/ARCHITECTURE.md`, the specs the feature touches and
`.zen/.agent/schemas/PLAN.schema.md`, then write `.zen/plans/PLAN-nnn-<name>.md` (the next free
number): goal, requirements, a short design sketch, phases (each one commit, tests passing),
and open questions. Keep it succinct and structured; no code beyond signatures. Reject the plan
if it does not fit the architecture, and say why.

Commit the plan file, then fire written with its planId.
