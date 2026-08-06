---
id: EV-029
date: 2026-08-06
provenance: ground-truth
source: owner, DEBATE-001 review (portfolio session 2026-08-06)
---
"We can do most testing with asterisk, which has already been integrated with audgent.
However, I can personally drive a real call phase with my own phone number. Further, later
audgent will serve lots of non-healthcare businesses, so we ought to setup a phase where we
knowingly discuss fake patients with rcda."

Three rungs between synthetic testing and real patients, revising DEBATE-001 J-2's boundary
from "no real calls pre-compliance" to **"no real patient data pre-compliance"**:

1. **Asterisk** — most testing runs over asterisk telephony (already integrated with
   audgent); real PSTN behavior without real callers.
2. **Owner-phone phase** — the owner personally drives real calls from their own number:
   real network, real audio, consenting caller, no PHI.
3. **RCDA fake-patient phase** — real calls with RCDA staff *knowingly discussing fake
   patients*: real office, real conversations, zero real patient data. Rationale: audgent
   will later serve many non-healthcare businesses, so a no-PHI operating mode is a lasting
   capability, not a throwaway.

Real patient data still begins only inside the compliant posture (EV-018, J-2).
