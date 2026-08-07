---
id: EV-031
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview follow-up 2026-08-06 (answering BRIEF-001 OQ-1)
---
"Standalone service"

Resolves OQ-1. voice.ronitnath.com is its own service and its own repo, not a slice of the
universe-ronitnath monolith. EV-008's stack ruling therefore means "built the same way",
not "built inside". Consequence: own Cargo workspace, own deploy, own database, own release
cadence; nothing in universe-ronitnath is a dependency and no coordination with its base-cut
sequencing is needed. Also makes graduation (EV-007) a relocation of a self-contained
service rather than an extraction, which is the cheaper path.
