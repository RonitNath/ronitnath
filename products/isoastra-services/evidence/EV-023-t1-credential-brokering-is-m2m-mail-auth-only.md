---
id: EV-023
date: 2026-08-06
provenance: ground-truth
source: owner, PM interview (voice follow-up)
---
"yes. Only credential brokering is M2M auth so services can send mail"

Confirms the reading of EV-005 against EV-015: the **application** principal class exists in T1, but
its only capability is sending mail, and the only credential this service brokers is the machine-to-
machine authentication an application uses to reach the mail API. Custody of upstream provider
credentials (SES keys stay held here; OpenRouter/Deepgram/Cartesia/Twilio scoped-key brokering) is
the service-gateway succession and remains out of T1. The application principal is the seat that
capability later grows into.
