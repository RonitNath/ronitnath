## The shared registries

Six files are edited by more than one leg, because they are the lists that make
a command or a query exist at all. They are **append-only**: add your line,
alphabetically inside the block it belongs to, and never reformat, reorder or
re-wrap a neighbour's line. A conflict in one of these is one line against one
line and the orchestrator resolves it in seconds; a conflict caused by
reformatting is not.

```
crates/api/src/commands/mod.rs           module decls + ALL_COMMAND_NAMES
crates/server/src/api/cmd/mod.rs         the bindings! table
crates/server/src/api/query/named.rs     Named + Scope + touched()
crates/server/src/api/query/mod.rs       Params fields
crates/server/src/api/query/platform.rs  the Platform enum
crates/kernel/src/event/mod.rs           the Event kinds (+ payload.rs)
docs/rebuild/plan.md                     §Model, §API, §Gate manifest
```

`crates/kernel/src/cmd/mod.rs` is P1's in wave 1 and append-only after it
merges: P2's product command adds one `mod` line and one `pub use` line to the
already-merged file. Migration files are never shared — P1 writes 7, P2 writes
8, P3 writes 9, and nobody edits an applied one.

---

