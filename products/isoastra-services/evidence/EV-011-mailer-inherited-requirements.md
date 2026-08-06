---
id: EV-011
date: 2026-08-06
provenance: inference
source: agent — accumulated findings from the mailer + mail-fabric work (2026-08-04..06); proofs in `resources/mail-fabric.md`, `isoastra/mailer` README/docs, DentConnex 429 defect
---
What a fleet mail service must get right, learned the hard way (each item has a concrete proof or
defect behind it):

- Retry-vs-terminal classification is where consumers actually break (the DentConnex 429 defect);
  the error vocabulary must be closed and documented.
- Suppression must be global, because SES account reputation is global.
- "Accepted with MessageId" is not "delivered" — the suppression-accepts-then-drops trap is real
  and must be visible to operators.
- Delivery pacing has a ceiling (current mailer paces below 14 sends/second).
- Every service should be born with a mock-shaped twin so consumers can test against it from day
  one; where the delivery hop speaks SMTP the adapter disappears entirely (EV-010).
- The current mailer's README trust-boundary section (one bearer value, no cookies on /v1,
  per-client tenant mapping, loopback-only operator listener, payload/metadata retention split)
  is good prior art.
