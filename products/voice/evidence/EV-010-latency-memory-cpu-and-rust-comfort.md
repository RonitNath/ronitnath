---
id: EV-010
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"In terms of Python, my objection is mainly that I want the system to run as low latency and low memory and low CPU as possible, and I don't think Python is giving me that option. I also would feel a lot more comfortable operating in a Rust code base than a Python code base."

Settles the open question in EV-002. The objection is resource efficiency first — latency,
memory, CPU, all three named — and operator comfort second. Not a hygiene preference.
Consequence: efficiency is a product requirement with observable form, so the metrics
surface (EV-013) must report latency *and* memory *and* CPU per component, not latency
alone; and the runtime architecture is judged on cost-per-call as much as on correctness
(EV-027). Second consequence: because the owner works in this codebase himself, legibility
of the Rust is a real constraint — clever concurrency that only an agent can maintain fails
the same test the console UI failed.
