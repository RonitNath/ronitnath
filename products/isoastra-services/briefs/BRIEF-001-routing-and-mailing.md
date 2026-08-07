---
product: isoastra-services
bet: 1 — services.isoastra.com T1 (routing + mailing)
date: 2026-08-06
interviewer: agent (Claude, PM session)
status: final — OQ resolutions written back from DEBATE-001 (2026-08-06)
confidence: high
---

The interview ran in one voice session plus a six-question follow-up. Every agenda topic is covered;
none was struck. Confidence is high on scope, principals, migration posture and the mailing model;
the residual uncertainty is concentrated in OQ-1 (how edge config is actually applied), which is a
design-gate question rather than a pitch-altitude one.

## Agenda coverage

| Topic | Status | Distillation |
| --- | --- | --- |
| Problem & shape of the hub | covered | Microservice fragmentation is the thing being avoided; one system, fewer components, is the sole-founder methodology (EV-012). |
| Users & access | covered | Three principal classes — owner, agents, applications — and RBAC differentiates none of them yet (EV-015, EV-004). |
| Routing domain | covered | Nanode-independent multi-edge route management; no DNS; no node onboarding (EV-013, EV-021, EV-022). |
| Mailing domain | covered | Centralized credential custody + HA + mode-per-registered-application driving the Stalwart test loop (EV-014, EV-017, EV-025). |
| Migration & coexistence | covered | Coexist indefinitely, touch no existing consumer, keep ingress-ctl as fallback (EV-018, EV-024). |
| Bound | covered | No bound; scope is the fixed side (EV-019). |
| No-gos | covered | service-gateway succession, DNS, node onboarding, employee role, consumer migration. |
| Success signals & risks | covered | Owner accepted the proposed signals; risks below. |

## Answers

### Problem — why one hub, why now

The bet reverses a prior direction. Splitting services out was meant to stop them from conflicting
and to contain failure, but the owner projected the endpoint and rejected it: mailing, SMS, ingress,
firewall, deployment configuration, STT, TTS, LLMs, embeddings — crossed with internal/external and
hosted/hardware — is twenty services each re-paying the same HA, self-healing and deployment tax,
each operated individually (EV-012). The insight came out of the universe-ronitnath rebuild. The
trade is stated and accepted: more coupling, hence more risk, in exchange for fewer components to
hold in one head.

Two concrete pains force it now. **Routing**: ingress-ctl runs on nanode and points at nanode, so
routing is a nanode SPOF; the owner wants any edge to serve, expects to spin up more public nodes,
and — most importantly — deploys products by telling an agent to deploy them, where registering the
route is part of what the agent figures out. Hand-maintaining routes across machines is the thing
being deleted (EV-013, EV-008). **Mailing**: the mailer is not HA and is a live SPOF for DentConnex's
mail today, while centralization is deliberately *kept*, because it is what stops SES credentials
from being copied into every service that sends mail (EV-014).

Rebuild rather than upgrade is settled, and the reason is the front end: the owner's standards for
service UIs and the design process itself have both moved past what an in-place upgrade carries
(EV-014).

### Users & access

Three principal classes (EV-015): the **owner** (the only human), **agents** (configuration,
debugging, forensics), and **applications** (plugging in for third-party service access — in T1 that
means sending mail). RBAC exists structurally from day one but differentiates nobody in the near
term (EV-004); the employee role is explicitly not designed now.

The identity seam is flagged by the owner rather than settled: agents use Kanidm today, the path
"should potentially be streamlined", and Kanidm is expected not to survive the universe rewrite. The
design must not weld itself to Kanidm (EV-015, OQ-2).

T1 credential brokering is exactly one thing: the M2M authentication an application uses to send
mail (EV-023). Upstream provider credential custody is the service-gateway succession, out of T1.

### Routing domain

Cluster membership is ruled: **nexus, sfo, nyc** (EV-016) — the same three nodes as the universe
reference deployment, and not nanode. This is the fleet's most availability-critical service: its
being down is an emergency, so failover drills are the acceptance test, the design degrades
gracefully rather than failing hard, and `unwrap`/`expect` panic paths are treated as defects.

Scope is narrower than the original handoff assumed, in three ways the owner cut explicitly:

- **No DNS.** The service touches Cloudflare not at all (EV-021). Georouting and load balancing
  remain a manual, separate concern. Multi-edge here means every edge can serve a route; which edge
  a visitor reaches is decided outside this system.
- **No node onboarding.** Bringing up a new public edge stays manual/Ansible. The requirement is
  that adopting one afterwards be *natural* — register the edge, and existing routes apply to it
  without per-route work (EV-022).
- **ingress-ctl is not decommissioned by this bet.** It is left to the side, kept available as the
  fallback (EV-018).

Data class is a genuine routing constraint: PHI traffic may traverse only DigitalOcean nodes, so the
route model carries a data class and placement cannot be edge-agnostic — the retired catalog's
`standard`/`phi` policy was solving a real problem (EV-020). The dentconnex.com unmanaged-zone
carve-out expires as that domain moves off Wix, and with DNS out of scope the unmanaged-zone concept
is moot anyway.

### Mailing domain

The mail service is centralized for credential custody, made HA because the current one is a SPOF,
and given a new contract rather than the old one.

The defining requirement is the **test loop**, which the owner names as a motivating case for the
whole rebuild (EV-017). An agent drives an application through real email flows — signup,
verification, password reset, support back-and-forth — with a Stalwart inbox it can both read and
send from. The old chain (mailer → SES → AgentMail → third-party API to read) is replaced. What the
current mailer was not built for is the part that matters: letting a downstream application express
whether it is in testing or production, and where its mail should go.

The mechanism is settled: **mode is a property of the registered application, not of the request**
(EV-025). This service holds an application registry; the application's identity determines its
delivery destination. Calling code is then identical across environments and cannot get its mode
wrong per-request. A live app and its staging twin are two registered applications with two
credentials — not one application with a flag. This discharges EV-010's "one path, change what it
points at".

Existing consumers are untouched (EV-024). DentConnex keeps talking to the old mailer for as long as
it runs; no consumer is migrated by this bet and no consumer repo is edited. The API is therefore
designed for what it should be, not for wire-compatibility with its predecessor — and its first real
user is a new or test-mode application.

Inherited requirements from the mailer's hard-won operating experience (EV-011) are available as
input but are not all T1 commitments: retry-vs-terminal classification as a closed error vocabulary,
globally-scoped suppression, accepted ≠ delivered, send pacing, and the metadata/payload retention
split.

### Migration & coexistence

Coexistence, not cutover (EV-018). Both mail services stay active; the old mailer retires only after
the universe rewrite completes, with no timeline. ingress-ctl is left in place as the fallback. This
removes the need for a big-bang activation gate, and replaces it with one risk the design owns:
**two writers**. Nothing may allow this service and ingress-ctl to fight over the same edge config,
or two mail services to send as the same identity (OQ-4).

Until cutover, the live nanode daemon, its ~29 routes, and DNS are not touched (EV-009).

### Bound

None (EV-019). Under SYS-DEC-001 the bound is normally the fixed side of the trade; the owner
declines one, so **scope is the fixed side** and the T1 fence does the work a bound would otherwise
do. Same posture as rinity EV-014. The owner UI is in T1 for both domains, not routing-first.

### No-gos

service-gateway succession and provider-credential brokering (EV-005, EV-023); Cloudflare/DNS
(EV-021); node onboarding automation (EV-022); the employee role (EV-004); migrating or editing any
existing consumer (EV-024); decommissioning ingress-ctl or the old mailer (EV-018).

### Success signals

Proposed by the interviewer, accepted by the owner ("pretty much what you expect"), adjusted for the
scope cuts:

1. A new public edge is registered and every existing route serves from it, with no per-route work.
2. An agent told to deploy a product registers its public route end to end without touching a
   machine by hand.
3. An application's mail is pointed at Stalwart by its registration alone, with no change to the
   application's code.
4. A full agent-driven signup → verification mail → verify → support back-and-forth loop runs
   against a test instance through this service.
5. A failover drill stays green with a cluster node killed mid-probe, and edges keep serving
   throughout.

### Risks / irreversibility

- **The hub thesis itself** is the expensive-to-reverse choice (EV-012). Coupling is accepted
  deliberately; if it goes wrong, the recovery is re-splitting, which is the direction just rejected.
- **Availability concentration**: this service becomes the fleet's most critical component while
  itself being new (EV-016). Its own failure modes must degrade, not outage — and the data plane
  (Caddy serving traffic, mail already accepted) must not depend on the control plane being up.
- **Two live writers** during indefinite coexistence (EV-018, OQ-4).
- **The identity seam** is expected to move under the service (EV-015, OQ-2).
- **PHI misplacement** is a compliance failure, not a bug (EV-020, OQ-5).

## Open questions

| ID | Statement | Why it matters | Blocking? |
| --- | --- | --- | --- |
| OQ-1 | How is edge config actually applied? Research (EV-026) found stock Caddy already ships the applicator: `caddy.config_loaders.http` pulls the edge's whole config from a URL over mTLS, keeps serving when the control plane is down, and needs no plugin or Go. Remaining question is whether to adopt it over the push alternatives. | Decides whether an edge self-heals when the cluster is unreachable (EV-016), whether the cluster must hold write credentials to every public node, and whether "agent deploys, route is live" is one act or two (EV-013). | n for the pitch; **y at the design gate** |
| OQ-8 | How do certificates work across many edges — shared CertMagic storage (needs a third-party storage module and an xcaddy rebuild), `tls.get_certificate.http` serving PEM from the control plane, or on-demand issuance gated by `tls.permission.http`? | Multi-edge (EV-013) is not achieved by route distribution alone: N edges each running ACME for the same hostnames duplicate issuance against a 50-certs-per-domain-per-week ceiling (EV-026). | n for the pitch; **y at the design gate** |
| OQ-2 | What identity substrate does the service depend on, given Kanidm is expected not to survive the universe rewrite? | An identity assumption baked into the authorization model is the expensive kind to unpick (EV-015). | n |
| OQ-3 | How does mail delivery work across three nodes — leader-only sender, or all nodes claiming from a replicated queue? What owns pacing and idempotency? | The old mailer's PG queue with `FOR UPDATE SKIP LOCKED` leases has no direct hiqlite equivalent; SES pacing is an account-global limit that three senders can violate independently (EV-011, EV-016). | n |
| OQ-4 | What fences this service and ingress-ctl from both writing the same edge, and the two mail services from sending as the same identity? | Indefinite coexistence makes this a permanent condition, not a migration window (EV-018). | n |
| OQ-5 | Is the PHI data class advisory metadata, or does the service refuse to place a PHI route on a non-BAA edge? | Advisory metadata that an agent can override is not a compliance control (EV-020). | n |
| OQ-6 | Does routing cover only public edges, or also mesh-facing/internal routes? | The fleet's `web_app_host` proxies are route-shaped too; including them widens T1 materially. | n |
| OQ-7 | How much extensibility scaffolding do the future domains (SMS, firewall, deploy config, STT/TTS/LLM/embeddings) justify now, with no T1 content? | EV-012 asks for room; premature module framework is the classic way to spend the bet on nothing shippable. | n |

---

## Open-question resolutions (written back from DEBATE-001, per `claim-schema.md`)

| OQ | Resolution | Where |
| --- | --- | --- |
| OQ-1 edge config distribution | **resolved** — `caddy.config_loaders.http`; renderer unconditionally embeds the loader block; push-on-commit added for immediacy without the data plane depending on the cluster | C-33, C-2, C-11, C-16 |
| OQ-2 identity seam | **deferred-as-assumption** — capability-scoped credentials issued by this service, with the identity provider behind a seam; not welded to Kanidm. Cheap to reverse while principals are owner+agents | C-26, C-36; EV-015 |
| OQ-3 delivery across three nodes | **resolved** — all nodes accept, hiqlite leader claims and sends, one cluster-wide rate below the SES ceiling; at-least-once with indeterminate outcomes recorded | C-35 over C-48, C-42, C-46 |
| OQ-4 two writers | **resolved** — authority epoch in fleet-owned edge config; fallback is a fenced transfer, never concurrent write access | C-39, C-5, C-30 |
| OQ-5 PHI enforcement | **resolved** — data class is an admission constraint, not advisory metadata: a PHI route is refused placement on a non-BAA edge and the agent is told which edges will carry it | C-31, C-36; EV-020 |
| OQ-6 routing scope | **deferred-as-assumption** — public edges only in T1. Mesh-facing/internal routes are not in the frozen scope | PITCH-001 no-gos |
| OQ-7 extensibility scaffolding | **resolved** — none. No module framework before a second domain exists; recorded as a rabbit hole | PITCH-001 |
| OQ-8 certificates across edges | **resolved (direction)** — per-edge durable material with independent renewal, settled before a second edge is activated; the control plane is never in the handshake path | C-45, C-17 |
