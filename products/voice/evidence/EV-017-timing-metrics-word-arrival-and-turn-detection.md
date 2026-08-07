---
id: EV-017
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"Also as part of this, we have lots of metrics on the exact timing and how long it took different words to arrive and when end of turn and end of sentence were detected."

Names the measurement obligation at rung 2: per-word arrival latency, and the detected
moments of end-of-turn and end-of-sentence. Consequence: the pipeline is instrumented at
the granularity of individual words and boundary decisions, not per call or per request —
which means timestamps are emitted by the components themselves and recorded against the
clip timeline (EV-016), never reconstructed afterwards. End-of-turn and end-of-sentence are
called out as separate detections, so turn boundary logic is an explicit, inspectable
component with its own measurable behavior rather than a threshold buried in the STT
adapter. This metric set is what makes EV-018's latency measurement possible.
