---
id: EV-024
date: 2026-08-06
provenance: ground-truth
source: owner, PM interview (voice follow-up)
---
"coexistence, don't touch other services"

Settles the consumer-contract question by removing it. This service is **not** bound to reproduce
the old mailer's `/v1` contract: existing consumers (DentConnex today) keep talking to the old
mailer, unchanged, for as long as it runs (EV-018). No consumer is migrated as part of this bet, and
no consumer repo is edited by it.

The mail API is therefore designed for what it should be, not for wire-compatibility with its
predecessor. Migration of an existing consumer is a later, per-consumer act — and the first real
user of the new API is a *new* or *test-mode* application, not a live one.
