# Spec: work packets — the layer between pitch and build

The pitch stays at problem altitude on purpose; the **packet layer** is where user stories, acceptance scenarios, UX flows, and edge states live. Packets are generated from the frozen pitch by a spec-writer role (never the interviewer), live in the **product's code repo** (`changes/<pitch-tag>/`), and are the contracts agents build against. The owner never has to prompt for this layer — deriving it is a mandatory pipeline station, and the coverage checklist below is what makes "peripheral" work mechanical instead of remembered.

## Packet format

```
PKT-## <slug>
story:        As <actor>, I <goal>, so that <reason>.     # one per packet, real actor from the pitch
acceptance:   Given/When/Then scenarios — falsifiable, testable, no snapshot tautologies
states:       empty / error / edge cases this packet owns
non-goals:    what this packet explicitly does NOT cover (point to the packet that does, or "out of bet")
constraints:  rulings + pitch clauses that bind it (cite IDs)
ux:           Penpot link or "no visual surface"
depends:      packet IDs
```

## Mandatory coverage checklist (run at derivation, record the result)

1. **Actors × journeys** — every flow in the frozen addendum (`flow-addendum.md`) maps to at least one packet; every actor in the pitch, including machine actors, has flows or is a named non-goal. Journeys are *checked* here, not invented here — a journey first noticed at this station means the flow addendum was incomplete, and it is fixed there by delta.
2. **States** — for every human-visible surface: empty state, error state, loading/latency, and the unauthorized/expired path. For every API: invalid input, auth failure, idempotency.
3. **Data lifecycle** — creation, mutation, export, deletion/retention, migration/rollback for every new table.
4. **Instrumentation** — how the owner will *know* it works in production (logs/metrics/audit), per packet or explicitly waived.
5. **UX artifacts** — human-visible surfaces get design work structured per `design-gate.md` (base screen, flows, breakpoints — prototyped, linked tokens), linked by packet ID. Where the product's code repo carries its own design procedure or token contract, that is what the boards transcribe. Reviewed at the design gate.
6. **Verification** — each acceptance scenario names how it will be evidenced (test, browser walk, query). CI rejects implementation PRs lacking packet IDs or acceptance evidence.

A checklist row may be waived, but the waiver is written on the packet set — skipping is a decision, never an accident (same principle as debate triggers).

## Delta discipline

Building reveals wrong packets. A packet change is a PR touching `changes/<pitch-tag>/` with a one-line reason; the pitch itself changes only by delta-PR. Silent rewrites are the failure mode this whole system exists to prevent.
