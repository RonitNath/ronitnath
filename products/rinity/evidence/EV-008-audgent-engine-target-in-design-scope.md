---
id: EV-008
date: 2026-08-06
provenance: ground-truth
source: owner, portfolio onboarding session 2026-08-06
---
"Audgent is still the expected target for the backend voice agent stack, and is in-scope for design."

Settles two things: (1) the engine plane is not up for re-selection by default — audgent
(`isoastra/audgent` + pipecat fork) remains the expected backend voice stack, reaffirming that
half of EV-006; (2) unlike a pinned external dependency, audgent is *inside* this bet's design
scope — flows, seams, and gates may reach into the engine, and the design gate can draw
surfaces that require audgent changes. The extraction pass therefore covers audgent's
capability surface, not just the control plane's.
