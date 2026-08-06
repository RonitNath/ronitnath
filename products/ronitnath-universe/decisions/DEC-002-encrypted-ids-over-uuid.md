---
id: DEC-002
date: 2026-08-03
---
Chose **derived encrypted ids** over stored uuid column + LRU resolve subsystem, because measured cost is trivial (~1µs/op node, ~20ns Rust) and it deletes a subsystem. Short codes added per-entity where humans copy ids; ULID rejected (longer than the id it replaces, leaks creation time). Evidence: EV-004. Irreversibility note: key custody/rotation breaks shared URLs — flagged for debate (T3).
