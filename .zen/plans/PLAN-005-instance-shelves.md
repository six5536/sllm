# PLAN-005: Instance scans that do not grow with history

| Meta               | Value                                         |
| ------------------ | --------------------------------------------- |
| Status             | completed (B1–B3, 2026-09-27)                 |
| Workflow direction | bottom-up (PLAN-004 D4-2 → code → specs)      |
| Traces to          | STO-1, STO-3, INST-3, INST-4, INST-9, STL-5, IDLE |

## 1. Goal

Completed instances are kept forever (INST-9), and every status line refresh, idle list, ref
lookup and new ref reads and parses every instance file of every machine. Make those reads cost
what the live instances cost, whatever the history. Behaviour is unchanged.

## 2. Requirements

| ID | Requirement | Traces |
| -- | ----------- | ------ |
| R-1 | The engine asks the store narrow questions: the instances with a status, the instance with a ref. Stores without an index answer by filtering `instances()` (default methods), so `MemoryStore` and the wasm host change nothing | NFR-1 |
| R-2 | The file store keeps live instances (active, suspended, parked) in `<machine>/open/`, completed ones in `<machine>/done/`; a status change across the two is one `rename` under the machine lock | STO-3 |
| R-3 | A ref is an entry `{ref: id}` in a marker file `<machine>/refs/<name>.json` (name: `safe(ref)` lower-cased, cut and hashed over 120 bytes, so case-only and long refs are safe), written under the lock with the instance; `instance_by_ref` reads one marker and checks the instance really has the ref (a marker left by a failed write is ignored) | INST-3, INST-4 |
| R-4 | An old layout (`<machine>/<id>.json`) is migrated on first access, under the lock: files move to their shelf, refs get markers, and `open/` appears last (renamed from `open.new/`), so a crash mid-way resumes | STO-1 |
| R-5 | `instance list`, `validate` and a corrupt file's report still see every instance, from both shelves | STO-1 |

## 3. Design sketch

```rust
trait Store {
    fn instances_with(&mut self, machine: &str, status: Status) -> Result<Vec<Instance>, HostError>; // default: filter
    fn instance_by_ref(&mut self, machine: &str, r#ref: &str) -> Result<Option<Instance>, HostError>; // default: find
}
```

- Engine: parked list and count → `instances_with(Parked)`; `idle::find` → `instance` then
  `instance_by_ref`; `check_set_ref` → `instance_by_ref` and `instance(machine, ref)`.
- `FsStore`: `shelf(status)`, `migrate(dir)` (fast path: `open/` exists, one stat),
  `put_instance`: lock → migrate → stored copy from either shelf → version check → new ref:
  clash when its marker names another instance that has the ref, or an instance's id is the ref
  → write marker → write the instance where it is, then rename it to its shelf → unlock.
- History files stay where they are.

## 4. Phases

| Phase | Items | Proof |
| ----- | ----- | ----- |
| B1 | R-1 (trait, engine) | engine tests unchanged; wasm size |
| B2 | R-2..R-5 (`FsStore`) | store tests: shelves, rename on completion, markers and a stale marker, migration of an old layout; session tests unchanged; strace: a status line opens no completed instance |
| B3 | DESIGN-STO, ARCHITECTURE change log, CHANGELOG, §5 | — |

## 4b. Decisions

- D5-1: a directory per status and marker files, not an index file: the layout is the index, so
  there is no second copy of the truth to drift after a crash (PLAN-004 D4-2 discussion).
- D5-2: markers are written under the machine lock the write already takes, so plain atomic
  writes suffice (no `create_new` needed).

## 5. Outcome

| Phase | Commit / result |
| ----- | --------------- |
| B1 | 3145421: `Store::instances_with` / `instance_by_ref` with filtering defaults; parked list, status count, `idle::find`, `check_set_ref` use them. Wasm 257.1 KiB |
| B2 | 14695b1: `store/instances.rs` (shelves, markers, `put` with rename, lazy migration); `instance show` by id then ref. Tests: migration of an old layout (a corrupt file lands in `open/`, history stays), completion moves the file, a parked list ignores `done/`, a stale marker does not hold its ref. On a copy of this repo's state, a status line opens only `open/` files |
| B3 | DESIGN-STO, DESIGN-ENG, ARCHITECTURE 0.5.0, CHANGELOG (restart after upgrading) |
| Double-check | Marker files were named by the ref as is: `GH-1` and `gh-1` would share one file on macOS and Windows (a duplicate ref could be made), and a long ref's name failed to write. Now a lower-cased, length-capped name holding a `{ref: id}` bucket. A read by id takes only the exact id (case-insensitive file systems), treats an over-long name as absent (an `enter` with a very long ref errored, in the old layout too), and re-reads `open/` after `done/` (a reopen racing a reader); full scans drop an instance seen on both shelves. Test: case-only refs, a 300-byte ref, a duplicate of `GH-1` refused |
| Triple-check | The empty ref (allowed when no pattern forbids it) named the `refs/` directory itself as its marker, so saving it failed: its marker is now `~`, a name `safe` never yields. Test: an empty ref saves and resolves |
