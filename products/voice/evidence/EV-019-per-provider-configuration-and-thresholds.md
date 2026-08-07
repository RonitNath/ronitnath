---
id: EV-019
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"There's also a lot of configuration to be done on a per provider basis. And so this is also a good place to be able to moratorium to test what are the thresholds as appropriate particular model from a particular provider, both on the speech to text side and the text to speech side."

[dictation artifact: "moratorium" appears to be a mistranscription; read as running a
matrix/comparison across providers. Confirm before it hardens into a requirement — OQ.]

Establishes that tuning is per provider *and* per model, on both the STT and TTS sides, and
that this system is the place to discover the right thresholds empirically. Consequence: the
product is a measurement rig as much as a runtime — provider/model configuration is data to
be swept and compared, so the same clip must be runnable across configurations with results
placed side by side. This is the mechanism behind EV-010's efficiency goal and EV-027's cost
question: thresholds are chosen from evidence, not from vendor defaults. It also implies
provider adapters exist early, but as instruments under test rather than as a breadth
feature (which is what audgent/EV-003 warns against).
