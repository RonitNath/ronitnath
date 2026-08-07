---
id: EV-014
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"In terms of how I see this evolving, the absolute base is being able to accept audio files, store them, and then replay them. This itself can have a pretty rich interface, and we can measure a lot of statistics from this."

Rung 1 of the ladder, and the answer to "where do the basics start" (EV-005): audio
ingest, durable storage, playback. No models, no realtime, no orchestration. The owner
explicitly expects this rung to carry a rich interface and real statistics of its own —
it is a deliverable he can judge, not a plumbing step to rush past. Consequence: the first
work packet is an audio library, and its acceptance is the owner exercising upload,
storage and replay and trusting them. Everything above depends on this being hardened
(EV-029), including replay-based testing of every later rung.
