---
id: EV-025
date: 2026-08-06
provenance: ground-truth
source: owner, audgent PM interview 2026-08-06 (flow review)
---
"The main configuration space is what providers to use."

Names what configuration actually means here: provider selection — which STT, LLM and TTS
services a workflow runs on. Consequence: of the configuration surface's many dimensions, the
one that carries real decision weight is provider choice, so it is what the readback view
(EV-014), the agent configure/test loop (EV-011), and the change history (F-5) are primarily
*about*. Also explains why wire-level diagnostics matter so much (EV-026): if the live
question is which provider to use, the evidence for answering it is how each provider actually
behaves on the wire.
