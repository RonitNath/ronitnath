---
id: EV-028
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"I'm only ever intending this to be a web based system. There's no need to plug into telephony at all. And when this becomes a real system that I want to put into production behind real applications, then I will, uh, build in the telephony layer, but this is just something that we don't need to think about and worry about at this point."

The one explicit no-go stated in this interview, despite topic G being struck (EV-030):
browser/web audio only, no telephony, and no design accommodation for it now. Telephony is
a later layer added when the system goes to production behind real applications.
Consequence: no SIP, no Twilio, no telephony codecs, no μ-law fixtures, no PSTN latency
budgets in this bet — and, read strictly, no speculative abstraction to "make telephony
easy later" either, since that is the overbuilding EV-009 rejects. The web transport is the
transport.
