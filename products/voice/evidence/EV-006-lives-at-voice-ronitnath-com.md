---
id: EV-006
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM-mode kickoff message 2026-08-06
---
"Let's build this as its own separate voice.ronitnath.com - this is what indicates its a testing mode service."

Fixes the deployment identity: a separate service on its own subdomain of the personal
domain, and the domain choice is itself the signal that the service is in testing mode. This
is the same doctrine the owner applies elsewhere — ronitnath.com is where things prove
themselves before they carry Isoastra's name. Consequence: separate repo, separate deploy,
separate identity/auth population from Isoastra; no Isoastra customer traffic and no
production obligations during this phase; and the "testing mode" label is load-bearing, so
the service may be freely broken, reset, and reshaped. Open: whether it is a slice of the
universe-ronitnath monolith or a standalone service alongside it — the phrase "its own
separate" points at standalone, to be confirmed.
