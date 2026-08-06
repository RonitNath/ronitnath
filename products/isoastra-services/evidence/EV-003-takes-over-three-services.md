---
id: EV-003
date: 2026-08-06
provenance: ground-truth
source: owner, bet-opening session
---
"This new service will take over from the mailer, from service-gateway, from ingress-ctl, and be a future central point for services."

The succession set is named: the mailer (isoastra/mailer — Kanidm-introspected bearer API, durable
PostgreSQL queue, SESv2 delivery), service-gateway (Isoastra/service-gateway — money-spending proxy
holding upstream provider credentials, scoped local keys, spend caps/audit), and ingress-ctl (route
management on nanode via the service-ingress daemon). "Future central point" means the architecture
must leave room for more service domains than the first cut ships (see EV-005 for the T1 fence).
