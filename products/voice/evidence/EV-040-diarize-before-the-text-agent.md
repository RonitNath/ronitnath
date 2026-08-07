---
id: EV-040
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview follow-up 2026-08-06 (answering BRIEF-001 OQ-10)
---
"diarize before text agent"

Resolves OQ-10 and places diarization concretely: a working diarizer lands on rung 2, before
the LLM turn of rung 3 — not merely speaker columns in the schema. Consequence: transcripts
are speaker-attributed before any model ever consumes one, so the LLM never sees an
undifferentiated stream and no turn logic is ever written against a single-speaker
assumption (EV-024's "not get confused"). It also makes diarization quality an owner-
greenlit deliverable (EV-038) with its own inspection surface on the timeline, rather than
an invisible preprocessing step.
