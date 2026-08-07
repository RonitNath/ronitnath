---
id: EV-025
date: 2026-08-06
provenance: ground-truth
source: owner, PM interview (voice follow-up)
---
"first" — in answer to: is testing mode a property of the registered application in this service
(register "dentconnex-staging, testing mode" once, and every send from it goes to Stalwart), or a
per-request flag the calling app sets?

**Mode belongs to the registered application**, not to the request. Consequences the build must
carry: this service holds an application registry, and an application's identity determines its
delivery destination (Stalwart for testing, SES for production). Calling code is then genuinely
identical across environments — the app never names its mode, and cannot get it wrong per-request.
This is the mechanism that discharges EV-010's "one path, change what it points at" and EV-017's
"downstream applications easily express whether they're in testing mode."

A live application and its staging twin are therefore two registered applications with two
credentials, not one application with a flag.
