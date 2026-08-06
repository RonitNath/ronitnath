---
id: EV-035
date: 2026-08-06
provenance: ground-truth
source: owner, requirement 5 (portfolio session 2026-08-06)
---
"on the admin dashboard, I need enough analytics surfaces to understand the cost per call,
what's driving costs, and projections for future costs per office"

The **operator (ISO) dashboard** must answer three cost questions: what a call costs, what
drives the cost (component breakdown — model, telephony, transcription, grading), and what
future costs per office project to. Per-office pricing (EV-024) makes unit economics the
margin lever; this is the surface that watches it. Engine already meters usage_info/cost_info
per run (D2-7) — the work is projection and attribution into rinity.
