---
id: EV-033
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview follow-up 2026-08-06 (answering BRIEF-001 OQ-3)
---
"filesystem, forever"

Resolves OQ-3. Audio lives on the filesystem, referenced from the database rather than
stored in it, and nothing is ever deleted. Consequence: the corpus is append-only and
permanent, which is what makes it a fixed measurement baseline — a clip recorded at rung 1
is still there to re-run against a rung-4 pipeline, and comparisons across months remain
valid. No retention policy, no cleanup job, no lifecycle states. Practical obligations that
follow: disk growth is monitored rather than managed, and the audio directory is the one
piece of state whose loss is unrecoverable, so it is what backups exist for.
