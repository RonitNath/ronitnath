---
debate: DEBATE-001
product: isoastra-services
bet: 1 — services.isoastra.com T1 (routing + mailing)
date: 2026-08-06
graph: DEBATE-001-t1-shape.yaml
panel: DEBATE-001-lenses/PANEL.md
status: awaiting owner — 3 escalations
---

# DEBATE-001 — memo

Generated from the claim graph. 5 axes, 10 lenses, 50 claims, all grounded, no void lenses.

**Round quality.** Every lens returned at least one claim it would stake the bet on, and every lens
cited real files — the prior mailer and the retired controller were read, not reasoned about. The
strongest findings in the round come from that code rather than from argument. The panel did *not*
produce agreement: five axes produced eleven `rebuts` edges, which is the round working.

**Judge is compromised.** I wrote BRIEF-001 and FLOWS-001; four of the fifty claims are defects in my
own flow addendum. That aggregation bias is unfixed and is why this goes to you rather than being
actioned.

---

## What the round found that nothing else would have

### 1. "Converged" is a lie the flows were about to ship

Four lenses on four unrelated axes — immediacy (C-13), mistake-blast-radius (C-38), reliability
(C-43) and maintainability (C-47) — independently found the same defect: **a successful config fetch
proves an edge received bytes, not that Caddy applied them or that the hostname serves.** My
FLOWS-001 used last-fetch as convergence evidence in F-01 and F-09. The status surface would have
shown an edge green on a generation Caddy rejected — turning the exact failure under review into a
false success.

Resolution takes E2's diagnosis and E1's facts: **delete the word "converged"**, and expose desired,
last-fetched, last-applied, last-probed and last-known-good as separate observable facts. Applied
comes from an edge-side attestation, not from the access log; publicly-verified comes from a probe
through the real public path asserting the expected application identity, not a generic health 200.

### 2. The six-hour silent mail drop is already latent in the code being replaced

E1 read `mailer/src/worker/mod.rs` and found the worker marks a message **sent** as soon as
`SendEmail` returns an id, while its SES event receiver is a fail-closed stub. That is EV-011's
"accepted ≠ delivered" as a live, shipped defect, not a hypothetical (C-41).

The successor must record an SES MessageId as **provider-accepted**, and every provider-accepted
message must reach a verified terminal event within a bounded interval or **raise an alert naming
the missing event** — a counter that names what it actually witnessed, in your standing phrasing.
E1 and E2 then agreed on the honest contract (C-42, C-46): a lease expiring after a send began but
before its outcome committed is **indeterminate**, never a silent retry, and delivery is
**at-least-once** — not exactly-once, which SES cannot participate in anyway.

### 3. Your "fewer components" ruling does not survive being read as "one process"

A1 argued one binary, one image, one schema. A2 rebutted it from the live prior art (C-6): the
mailer starts its worker **inside** `Application`, giving the combined site one CPU, PID, OOM and
restart boundary — so an SES retry storm takes route-serving down with it. That is precisely the
coupling risk you accepted at the product level (EV-012) landing somewhere you did not rule on.

**Resolution: one repo, one image, one release, two runtime roles** — a route-plane container and a
mail-worker container per node, same artifact, different command. EV-012's "fewer components to
manage" is preserved (one thing to build, version and deploy); the shared-fate failure is not.

A2 pushed further and wanted **separate hiqlite clusters** per domain (C-7). Rejected for T1 — two
Raft clusters double the surface E2 has to debug at 3am, and queue contention is bounded instead by
A2's own admission budgets (C-9). Recorded with its reversal trigger.

### 4. Push and pull were never actually opposed

B1's case (C-11) is real: pull-only makes activation latency equal to the polling interval by
construction, and the owner can watch a valid route fail for a whole window with no action but
waiting. B2's case (C-16) is also real: an edge must boot from durable local state and serve
everything over TLS with the cluster blackholed.

These do not collide. **Push on commit for immediacy, periodic pull as reconciliation, and edge
autonomy as the invariant.** Pushing while the cluster is up costs nothing to a design that never
depends on the cluster being up.

B2 then closed two options EV-026 had left open, and both closures matter:

- **`tls.get_certificate.http` pointed at services.isoastra.com is now disqualified** (C-17) — it
  puts the control plane in the TLS handshake path, which is the exact dependency the design exists
  to remove. Certificates must be locally durable per edge with independent renewal (C-45), decided
  **before** a second edge is activated. OQ-8 has a direction.
- **Dynamic upstreams resolved from the central service are disqualified** (C-18). Every activated
  generation must be self-contained.

### 5. Velocity and safety resolved together, not against each other

D1 wanted "deploy this product" to be one idempotent `ensure-public-route` call that infers replica
endpoints, defaults data class to public and health to `/healthz`, orders upstreams by measured
latency per edge, and returns one operation URL that completes only on probe success (C-31). D2
wanted agents unable to touch any hostname they were not authorized for (C-36).

Both hold: **the call stays one call; the grant names one exact hostname and owner.** Wildcard,
apex, catch-all, raw-config and edge-adoption are simply unavailable to autonomous agent
credentials. D2 also found the case hostname-uniqueness cannot catch (C-37) — a config that passes
every parser while **shadowing** a wildcard or path route — so non-interference against the prior
generation becomes an admission invariant.

The one place D2 beat D1 outright is mode (C-40 over C-32). D1 wanted deployment mode mapped to
production/testing automatically. D2 showed both directions are catastrophic: a staging credential
registered as production sends test content through SES; a production credential registered as
testing diverts **all live mail** into the disposable fabric. **Mode is immutable, agents self-serve
testing registrations freely, and production enrollment needs an owner step-up.** That also answers
F-05's open question — changing mode mints a new application.

### 6. Open questions the round closed

| OQ | Closed by | Answer |
| --- | --- | --- |
| OQ-1 applicator | C-33, C-2 | `caddy.config_loaders.http`, with the renderer unconditionally embedding the loader block so EV-026's deaf-edge trap cannot be reached |
| OQ-3 delivery across nodes | C-35 over C-48 | Leader-owned sending, one cluster-level rate value; reuses hiqlite's existing election rather than adding a limiter |
| OQ-4 two writers | C-39, C-5, C-30 | An authority epoch in fleet-owned edge config; fallback is a fenced transfer, never concurrent write access |
| OQ-8 certificates | C-45, C-17 | Per-edge durable material and independent renewal, settled before a second edge exists |
| F-02 rollback | C-50 + C-19 | No control-plane rollback state machine — forward edit plus audit; the *edge* keeps active+previous artifacts |
| F-09 drills | C-49 | The drill stays an external operator play; no scheduler or host credential in the app |

Also settled: secrets are the one carve-out from "everything in one store" — **an SES key cannot
live in hiqlite** (C-23), because DEC-013 replicates it to three members and into the backup path.
And the live mailer loads AWS credentials once at startup (C-24), so rotation needs a versioned
reloadable handle, not an env-file swap.

---

## Escalations — three, batched, owner-only

Each trades one of your stated values against another. No agent can settle them.

### E1. Does the old mailer keep its own SES credential? (C-21)

**The collision.** EV-014 makes single-point credential custody a reason this product exists. EV-024
and EV-018 say don't touch existing services and keep the old mailer running indefinitely as
fallback. Both cannot hold: indefinite coexistence means **two independent SES custody boundaries
for an unbounded period**, so rotation is a coordinated two-system operation and no single principal
ledger explains a provider-side event.

C1's remedy is to keep the old mailer's contract as a facade that enqueues into the new service
under a legacy principal and strip its SES access. That contradicts EV-024 directly.

**Options:** (a) accept dual custody for the coexistence period, with rotation documented as a
two-system act; (b) facade the old mailer, which means touching it — contradicting EV-024; (c) bound
coexistence for *mail only* with a decommission condition, leaving ingress-ctl's fallback untouched.

My read: **(a)**, with the cost recorded. It is the only option that respects EV-024 as written, and
EV-018 already accepts the old mailer running. But this is your custody ruling, not mine.

### E2. Do edges verify a signature, or trust the cluster? (C-27)

**The collision.** C2 (forbidden from arguing "it's just me and my agents") showed that if edges
accept unsigned whole configs, **compromise of any one cluster node repoints every hostname at the
next poll**, while that same node falsifies its own audit trail and status views. DNS is untouched;
users reach familiar TLS hostnames while traffic is proxied elsewhere. Its remedy: generations signed
by a signer outside the web service and hiqlite, key pinned at the edge, with owner step-up for
protected hosts, wildcards and multi-route diffs.

E2 (forbidden from arguing a mechanism is inherently worth its complexity) is the standing objection:
that is another key, another signer to operate, another thing broken at 3am.

**Options:** (a) unsigned, trust the cluster — smallest, and the cluster is already the most
privileged thing you own; (b) sign everything, key held outside the service; (c) sign only protected
hostnames — apexes, wildcards, identity and PHI routes — leaving ordinary single-route updates
unsigned.

My read: **(c)** is the shape that survives both mandates, but it is genuinely a judgment about how
much you fear cluster compromise versus 3am complexity, and I do not think an agent should make it.

### E3. Is there an audit sink outside this cluster? (C-29)

**The collision.** EV-027 says you are almost never watching, and open the UI only once something
already looks wrong. C2 observes that the UI, the hiqlite audit table and the metrics are all
controlled by the service that might be the thing compromised — so the primary detector for the
fleet's most critical service is a surface with no human behind it, produced by the suspect.

Its remedy is exporting authentication decisions, route diffs, applied digests, mail acceptance and
delivery results to an append-only sink **outside** the cluster, with external probing of protected
hostnames and independent ingest of SES-side events.

E1 supports the shape from a different angle (C-44): a dashboard inside a failed quorum cannot be the
alarm for its own failure.

**Options:** (a) in-cluster audit only; (b) export everything to an external append-only sink;
(c) export only the dangerous-action set — credential changes, protected-host diffs, production
grants, edge digest mismatches — and page on those.

My read: **(c)**, and it partly exists — the observability plane already runs on alien/tor,
independent of nexus/sfo/nyc. This may be smaller than it sounds.

---

## What would change the recommendations

- **C-7 (separate Raft clusters)** reopens if mail load is measured interfering with route-write
  commit latency.
- **C-35 (leader-owned sending)** reopens if a throughput requirement appears that one sender cannot
  meet; C-48's static per-node budget is the cheap fallback.
- **C-28 (per-application signed grants)** was deferred assuming the service is honest. If E2 above
  resolves toward signing, the machinery exists and this should be reconsidered.
- **The whole panel** rests on axes I derived. A blind spot shared by my derivation and the evidence
  survives the round untouched — which is the honest reason this memo goes to you.
