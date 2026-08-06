---
id: DEC-005
date: 2026-08-05
supersedes: EV-014's base-screen ruling; amends PITCH-001 §4 via delta-3
---
Chose a **hard line between platform-canonical surfaces and per-event compositions**, because with EV-015 and EV-016 both the guest-facing page *and* the owner-facing admin panel are authored per event, and a product where every screen is bespoke has no design language to lock and no product to build.

**Canonical (platform owns the layout, the design gate locks it):**

| Surface | Why it can't be per-event |
| --- | --- |
| Sign-in | One owner, one credential, no event context exists yet |
| Event index | Spans events by definition |
| People (longitudinal) | One row per human *across* events — the thing three separate codebases destroyed |
| Link management | Token mechanics are identical everywhere; only what a link *shows* varies |
| The component library | The vocabulary every bespoke composition draws from — the real deliverable |

**Per-event (the agent authors it in code, the platform only supplies parts):** the invite page (per *invite*, EV-014), the event's admin panel (EV-016), and any event-specific custom component.

**Split of authority over content**: code owns structure and identity; a per-event `copy.toml` owns wording; the database owns only what exists at runtime — guests, RSVPs, links, publish state. The console never edits structure or wording; it reads and writes runtime data.

**Consequences taken deliberately:**

1. The design gate's base screen moves off the per-event admin overview onto a canonical surface — the **people view** is the candidate: it is the densest genuinely platform-level screen and it is the one surface whose absence caused real loss (EV-013). Owner picks at the gate.
2. The **component library becomes the primary build artifact**, not a byproduct. If it is thin, every event is a rebuild again and the bet has failed on its own terms (EV-011).
3. PKT-04's agent API is promoted from creation convenience to the contract that every bespoke panel reads and writes through.
4. Page-doc props stop carrying wording (it moves to TOML), which changes the write-time validation story PKT-03 assumed. Data-gate question.

**What would change this**: if per-event admin panels turn out in practice to be the same panel with different tiles, the composition seam is over-built and the canonical set should absorb them. Revisit after two real events, not before.
