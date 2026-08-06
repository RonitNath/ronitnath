---
id: EV-003
date: 2026-08-06
provenance: ground-truth
source: owner, audgent PM-mode kickoff message 2026-08-06
---
"be able to leverage cutting edge systems (such as force aligned phonetics)"

Makes capability frontier an explicit product goal, not an optimization. Forced phoneme
alignment is named as the exemplar: it implies audgent must be able to reach inside the audio
plane at sub-word granularity (phone-level timing against a transcript), which the current
provider-abstraction pipeline — STT text in, TTS audio out, per-provider services — is shaped
to hide. Consequence: provider breadth (EV-011/AC-04's dozens of interchangeable STT/LLM/TTS
vendors) may be an obstacle rather than an asset, since the lowest-common-denominator
interface is what forecloses frontier techniques. The bet likely includes deepening a narrow
local/controlled path over preserving broad hosted-vendor parity. Open: what the frontier
capabilities are *for* — timing/prosody control, diagnosis, evaluation, or something else.
