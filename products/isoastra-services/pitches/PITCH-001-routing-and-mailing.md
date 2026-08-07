---
id: PITCH-001
product: isoastra-services
date: 2026-08-06
status: frozen
tag: pitch/isoastra-services-1
brief: BRIEF-001
flows: FLOWS-001-routing-and-mailing.md (addendum, frozen under this tag)
debate: triggered (T3: key custody + identity model + API commitment; T2: no bound) → DEBATE-001
---

# services.isoastra.com — T1: routing and mailing

## Problem

Every internal capability is becoming its own service, and each one re-pays the same tax: its own HA
story, its own deploy, its own credentials, its own operating surface. Projected forward — mailing,
SMS, ingress, firewall, deployment config, STT, TTS, LLMs, embeddings, internal against external,
hosted against hardware — that is twenty services doing the same work twenty times, each managed
individually. For one founder that is the wrong shape (EV-012). The trade is accepted with its cost
named: more coupling, hence more risk, in exchange for far fewer things to hold.

Two of those capabilities hurt now.

**Routing.** `ingress-ctl` runs on nanode and points at nanode, so public routing is a single point
of failure (EV-013). Adding an edge means hand-maintaining routes across machines (EV-008), and the
common caller is not a human — it is an agent told "deploy this product", for which registering the
route is part of figuring it out.

**Mailing.** The mailer is not HA and is today a live SPOF for DentConnex's mail (EV-014).
Centralising it is still right — it is what keeps SES credentials out of every service that sends —
but it was never built for the workflow that matters most: letting a downstream application declare
that it is in testing, so an agent can drive a real signup → verification → reset → support loop
against a real inbox (EV-017, EV-010).

## Appetite / bound

**None.** The owner declines a bound (EV-019). Scope is therefore the fixed side of the trade, and
the T1 fence below does the work a bound would otherwise do. This is a deliberate departure from
SYS-DEC-001's usual shape, matching rinity's posture.

## Solution

One Rust/Axum/Leptos application on hiqlite, replicated across **nexus, sfo and nyc** (EV-016), on
the universe-ronitnath stack (EV-002). One repo, one image, one release — but **two runtime roles**,
a route plane and a mail worker, as separate containers per node (DEBATE-001 C-6). The live prior art
runs its mail worker inside the same process as its HTTP surface, which is exactly how an SES retry
storm takes route serving down with it. One thing to build and deploy; two things that can fail
independently.

### Routing

Desired route state lives in hiqlite. Each edge's complete Caddy configuration is rendered from it
and served at a per-edge endpoint. **Edges apply it themselves using stock Caddy's
`caddy.config_loaders.http`** — no plugin, no custom applicator, no Go (EV-026, C-33). The renderer
unconditionally embeds the loader block with a positive `load_delay`, because a generation missing it
makes that edge permanently deaf.

The data plane never depends on the control plane. An edge boots from durable local state and serves
every previously active hostname over TLS with the cluster blackholed (C-16); every generation is
self-contained, with no callback to the service for upstreams, permissions or certificates (C-18);
and the edge retains active plus previous artifacts to fall back on (C-19). For immediacy, commit
also pushes (C-11) — but only as an optimisation over a design that assumes it will fail.

Routes carry a **data class**; PHI may traverse only BAA edges (EV-020). Registration is one
idempotent `ensure-public-route` call that infers replica endpoints, defaults health to `/healthz`
and orders upstreams by measured latency per edge (C-31) — while the grant behind it names one exact
hostname and owner, and non-interference against the prior generation is an admission invariant, so
an agent cannot shadow a wildcard or apex it was never authorised for (C-36, C-37).

Nothing is ever reported as "converged". Desired, last-fetched, last-applied, last-probed and
last-known-good are separate facts; **serving** means a probe through the real public path succeeded
(C-13, C-43, C-47).

### Mailing

Applications register once, and **mode is a property of the registration, not the request**
(EV-025) — so calling code is identical in every environment and cannot get its mode wrong.
Mode is **immutable**: agents self-serve testing registrations freely, production enrollment takes an
owner step-up (C-40). Testing delivers into Stalwart, production into SES.

Messages are accepted durably, then delivered by the hiqlite **leader only**, under one cluster-wide
rate below the measured SES ceiling (C-35). An SES MessageId means **provider-accepted**, never
delivered; every provider-accepted message must reach a verified terminal event within a bounded
interval or raise an alert naming the missing event (C-41). Delivery is **at-least-once**, and an
attempt whose outcome could not be committed is *indeterminate*, never silently retried (C-42,
C-46). Acceptance is bounded by admission, retry, concurrency and storage budgets, so an SES outage
cannot become a local resource attack (C-9). Errors travel in one closed, versioned envelope across
both domains that tells a caller what to do next — the DentConnex 429 defect is what that exists to
prevent (C-34, EV-011).

### Access and trust

Three principal classes — owner, agents, applications (EV-015). RBAC exists structurally though it
differentiates nobody yet (EV-004); the employee role is not designed now. Credentials are scoped
capabilities, never a union token: an application may only send mail as itself; an agent may create
and update its own routes but never touch wildcards, apexes, catch-alls, raw config or edge adoption
(C-26, C-36).

**Edges trust the cluster; generations are unsigned** (EV-029). The exposure is accepted by name:
compromising a cluster node can repoint every hostname. The compensating controls are capability
scoping and an **append-only audit sink outside the cluster** for the dangerous-action set (EV-030) —
which matters precisely because the owner is rarely watching (EV-027) and the suspect service would
otherwise own every record of its own behaviour.

The SES key never lives in hiqlite — replication and backups would multiply custody (C-23) — and
replicas exchange workload identity for short-lived sessions (C-22).

### Surfaces

The **machine API is the operating surface**. The owner UI is inspection-first: what configuration
exists, what is going on right now, mail volumes (EV-027). Every diagnostic view has a
machine-readable twin, because agents diagnose more often than the owner does. Twelve flows are
enumerated in the addendum and frozen with this pitch.

## Rabbit holes

- **Rendering per-edge Caddy config.** Bounded by serving Caddyfile text and letting Caddy's adapter
  do the rest — the artifact stays reviewable and we write no JSON config model.
- **Certificates across edges.** Real and unsolved: N edges each running ACME for the same hostnames
  duplicate issuance against a 50-per-domain-per-week ceiling. Direction is per-edge durable material
  with independent renewal, decided **before** a second edge is activated (C-45, C-17). Do not put
  the control plane in the handshake path.
- **Mail lifecycle completeness.** SES event ingest is the part the current mailer never finished.
  It is in scope; drifting into full deliverability analytics is not.
- **The extensibility trap.** EV-012 asks for room for future domains. Building a module framework
  before a second domain exists spends the bet on nothing shippable (OQ-7).

## No-gos

- **service-gateway succession** — provider credential custody and scoped keys for model hosting
  (EV-005). T1 brokers exactly one thing: M2M auth so services can send mail (EV-023).
- **DNS.** No Cloudflare, no records, no georouting (EV-021).
- **Node onboarding.** New edges come up manually; adopting one afterwards must be cheap (EV-022).
- **The employee role** (EV-004).
- **Migrating or editing any existing consumer.** DentConnex keeps talking to the old mailer
  (EV-024), which keeps its own SES credential (EV-028).
- **Decommissioning ingress-ctl or the old mailer.** Both stay live indefinitely; ingress-ctl is the
  routing fallback, fenced by an authority epoch so the two can never both write an edge (EV-018,
  C-39).
- **Signed generations, external signer, cryptographic step-up gates** (EV-029).
- **Separate hiqlite clusters per domain** (C-7).
- **Any rollback state machine** in the control plane (C-50).

## Success signals

1. A new public edge is registered and every applicable route serves from it, with no per-route work.
2. An agent told to deploy a product registers its route end to end without a human touching a
   machine — and "done" means a public probe passed, not an API 200.
3. An application's mail is pointed at Stalwart by its registration alone, with no code change.
4. A full agent-driven signup → verification → reset → support loop runs against a test instance.
5. A failover drill stays green with a cluster node killed mid-probe; a **cross-domain** drill wedges
   mail while public routes keep serving, and the reciprocal (C-10).
6. Every services.isoastra.com address is blackholed for forty minutes while every edge is rebooted
   in turn, and public service is continuous throughout (C-20).

## Debate record

`debate: triggered (T3: key custody + identity model + API commitment; T2: no bound) → memo DEBATE-001`

10 lenses / 5 axes / 50 claims, all grounded, no void lenses. Four defects in the flow addendum were
found and corrected before this freeze. Three escalations were answered by the owner and recorded as
EV-028, EV-029 and EV-030.
