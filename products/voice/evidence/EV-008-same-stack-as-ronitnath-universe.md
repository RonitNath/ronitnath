---
id: EV-008
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM-mode kickoff message 2026-08-06
---
"Same tech stack as ronitnath-universe"

Settles the stack by reference: Rust + Leptos (islands mode) + Axum, per the 2026-08-05
ruling in `~/dev/context/ronitnath/platform-scope.md`, with PostgreSQL as the ruled data
seam (plain SQL, no ORM) and push-to-`deploy` CD across the sfo/nyc/nexus nodes. Not
re-litigable. Consequence: the language question from EV-002 is closed (Rust), and the
human-facing surface follows universe conventions rather than inventing its own. Open for
the interview: whether the realtime audio plane sits in the same binary as the Leptos web
tier or is a separate crate/process the web tier observes, and whether hiqlite (as chosen
for isoastra-services) or Postgres is right for a testing-mode single-node service.
