---
id: EV-024
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"Also, later, the system will be used conference setting. So it will be additionally useful to do diarization early and be able to tell the difference between different speakers. In the first place, this is useful in order to also not get confused."

Multi-party conversation is an anticipated use, and speaker separation is wanted *early*
for two reasons: to be ready for conferences later, and immediately to stop the system
confusing one speaker for another. Consequence: speaker identity is a dimension of the data
model from the start — transcripts, turns and signals are attributed, not assumed to belong
to a single caller — even while every early rung runs with one speaker. Retrofitting speaker
attribution after turn logic is built around a two-party assumption is the expensive path
this ruling avoids.
