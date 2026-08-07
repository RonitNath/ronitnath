---
product: isoastra-services
bet: 1 — services.isoastra.com T1 (routing + mailing)
date: 2026-08-06
status: draft — freezes with PITCH-001
addendum-to: PITCH-001 (unwritten)
brief: BRIEF-001
---

# Flow addendum — T1 routing + mailing

Actors, from BRIEF-001 / EV-015: **owner** (the only human), **agent** (configuration, debugging,
forensics), **application** (a registered machine consumer; in T1 its only capability is sending
mail, EV-023). Every actor carries at least one flow.

Named non-goals with deliberately zero flows: **employee** (EV-004), **end recipient of mail** (never
touches this system), **service-gateway consumer** (EV-005).

**Surface primacy (EV-027).** The machine API is the operating surface; the UI is an inspection
surface the owner uses rarely — when something has gone seriously wrong, when he wants to see what
configuration exists, or out of curiosity. So where a flow exists in both forms, the agent path is
the primary one and the owner path is the occasional one, and every diagnostic view carries a
machine-readable equivalent because agents diagnose too. A flow marked `surface: web` below is not
thereby the common path — it is the one a human takes on the rare occasion he takes it.

Twelve flows. `PKT-##` references are forward-looking — packets derive from this set after the pitch
freezes (`work-packets.md` rule 4).

---

## F-01  Adopt a new public edge

```
actor:    owner
trigger:  a new public node has been brought up manually (Ansible/fleet) and needs to serve traffic
outcome:  the edge serves every route that applies to it, with no per-route work
surface:  outside-web → web
```

**steps**

1. *(outside-web)* Owner brings the node up by the existing manual fleet process — provisioning,
   mesh enrollment, Caddy installed. This service does not do this (EV-022).
2. *(outside-web)* The node's Caddy is pointed at this service as its config source, and given the
   credential it will authenticate with.
3. *(web)* Owner registers the edge in the UI: its identity, address, provider, and **data class**
   (whether it may carry PHI — EV-020).
4. *(web)* The service shows which existing routes now apply to this edge, before anything is served.
5. *(machine)* The edge fetches its config and begins serving.
6. *(web)* Owner sees the edge reporting itself as converged, on the config version it was given.

**branches**

- Edge registered but never fetches → it appears registered-but-silent, distinct from converged.
- Owner registers a non-BAA edge → routes carrying the PHI data class are excluded from it, and the
  exclusion is shown as a fact about that edge, not hidden.

**states**

- *empty*: first edge ever registered — no routes exist yet to apply.
- *error*: the edge cannot authenticate; must read as a credential problem, not "edge down".
- *conflict*: an edge is registered that is already being written by ingress-ctl (EV-018 two-writer
  risk) — the service must say so rather than silently becoming a second writer.

**packets**: PKT-01 (edge registry), PKT-02 (config rendering + serving endpoint), PKT-06 (owner UI —
edges)

**open**: Does step 2's credential come from this service (issued at registration) or from the fleet
process? Does an edge self-register on first contact, or must it pre-exist? (data gate)

---

## F-02  Add or change a route

```
actor:    owner
trigger:  a service needs a public hostname, or its upstreams changed
outcome:  every applicable edge serves the new route; the previous version is recoverable
surface:  web
```

*The occasional path (EV-027). The common one is F-03, where an agent does this over the API.
This flow exists for when the owner is in the UI anyway and wants to change something directly —
which by EV-027 usually means something has already gone wrong.*

**steps**

1. *(web)* Owner names the hostname and its upstreams (one, or several for an HA route with a
   required health check — the multi-upstream shape ingress-ctl already has).
2. *(web)* Owner sets the route's data class, which determines which edges may carry it (EV-020).
3. *(web)* Owner sees **what will change, per edge**, before committing — the rendered difference,
   not a promise.
4. *(web)* Owner commits. The service records a new config version.
5. *(machine)* Edges fetch and converge, each at its own pace.
6. *(web)* Owner watches each edge move onto the new version; the flow is done when they all have.

**branches**

- Editing an existing route follows the same path; the difference in step 3 is what distinguishes it.
- A route the owner marks as applying to a subset of edges rather than all.

**states**

- *empty*: no edges registered — a route can be defined but nothing serves it, and that must be
  visible rather than looking successful.
- *error*: the rendered config is invalid; caught before any edge is offered it, never after.
- *conflict*: the hostname is already served by an edge outside this service's control (a
  hand-written block, or ingress-ctl) — refuse or require an explicit override, per EV-018.
- *partial*: some edges converged, some have not. This is a normal steady state for a while, not an
  error, and must not read as one.

**packets**: PKT-03 (route model + data-class placement), PKT-04 (config version + rollback),
PKT-07 (owner UI — routes and per-edge diff)

**open**: Is a route version rollback a first-class action, or is it re-editing forward? What is the
unit of versioning — the whole desired state, or per route? (data gate)

---

## F-03  Agent deploys a product and registers its route

```
actor:    agent
trigger:  owner tells an agent to deploy a product
outcome:  the product is publicly reachable, without a human editing any machine
surface:  machine-only
```

This is the flow EV-013 is about — the reason routing is being rebuilt.

**steps**

1. *(machine)* Agent deploys the product to wherever it is going, by the ordinary deployment
   procedure. Out of this service's scope.
2. *(machine)* Agent authenticates to this service and registers the route: hostname, upstreams,
   data class.
3. *(machine)* The service validates and records it; edges converge.
4. *(machine)* Agent confirms the hostname actually serves — through the public path, not by trusting
   the API's 200.
5. *(web)* The owner later sees the route in the UI as a thing an agent created, attributable.

**branches**

- The route already exists (a redeploy) → an update, and must be idempotent: an agent that retries
  must not create a second conflicting route. (The comms M3 lesson: a retried create refused for
  taking the name its own first attempt took.)
- The agent registers a PHI-class route → placement is constrained to BAA edges, and the agent is
  told which edges will carry it, not silently given fewer.

**states**

- *error*: agent lacks authorization for routing → must be distinguishable from a malformed request.
- *conflict*: hostname taken by another product.
- *unconverged*: registered but no edge has picked it up yet; step 4 must not pass on registration
  alone.

**packets**: PKT-05 (machine API + authz), PKT-03, PKT-08 (attribution/audit)

**open**: Do agents and applications share one credential model, or are they distinct principal
kinds at the protocol level (EV-015)? What does the agent present given Kanidm is a moving seam
(OQ-2)? (data gate)

---

## F-04  Retire a route

```
actor:    owner
trigger:  a product is decommissioned or moves
outcome:  no edge serves the hostname; the record of it having existed survives
surface:  web
```

**steps**

1. *(web)* Owner selects the route and sees what currently serves it and where.
2. *(web)* Owner confirms retirement, seeing the per-edge effect first (as F-02 step 3).
3. *(machine)* Edges converge; the hostname stops being served.
4. *(web)* Owner sees it gone, and can still find that it existed and when it was removed.

**branches**

- Retire on some edges only (moving a product between edges) rather than everywhere.

**states**

- *error*: an edge cannot converge, so the route is still live somewhere after retirement — this must
  page the owner's attention, because a route believed dead and still serving is the dangerous
  direction of that failure.

**packets**: PKT-03, PKT-04, PKT-07

**open**: What happens to a certificate for a retired hostname (OQ-8)?

---

## F-05  Register an application and its mode

```
actor:    owner
trigger:  a product needs to send mail — a new product, or a staging instance of one
outcome:  the application holds a credential; its mail destination is fixed by its registration
surface:  web → outside-web
```

The mechanism EV-025 rules: **mode belongs to the registered application, not the request.**

*Agents do this as often as the owner does (EV-027) — a new staging instance is an agent's act. The
steps below are the owner's; the agent's is the same sequence over the API, and step 4 is where the
agent has the advantage, because it is already holding the application's configuration.*

**steps**

1. *(web)* Owner registers the application: its name, and whether it is **production** (delivers via
   SES) or **testing** (delivers into Stalwart).
2. *(web)* Owner sets what it may send as — its sender identity.
3. *(web)* The service issues the application's M2M credential (EV-023). It is shown once.
4. *(outside-web)* Owner (or an agent) places the credential in the application's configuration.
5. *(machine)* The application's first send confirms the registration works.

**branches**

- Registering the staging twin of an existing production application — deliberately a **second**
  registration with its own credential, never a flag on the first (EV-025).

**states**

- *error*: the sender identity is not one this service can send as → refuse at registration, not at
  send time, so the failure lands on the owner rather than on a user's signup.
- *empty*: registered but has never sent — visibly distinct from registered-and-working.

**packets**: PKT-09 (application registry + mode), PKT-10 (M2M credential issuance), PKT-12 (owner
UI — applications)

**open**: Can an application's mode be changed after registration, or is a mode change a new
registration? (Changing it silently redirects live mail — the safer answer may be no.) (data gate)

---

## F-06  An application sends mail

```
actor:    application
trigger:  a user action in that product — signup, password reset, a notification
outcome:  the message reaches its destination for that application's mode, or fails legibly
surface:  machine-only
```

**steps**

1. *(machine)* The application calls this service with the message. Its code is identical in every
   environment — it names no mode and no destination (EV-010, EV-025).
2. *(machine)* The service authenticates it, resolves its mode, and accepts the message durably.
3. *(machine)* The service delivers — SES for production, Stalwart for testing.
4. *(machine)* The outcome is recorded against the message: delivered, deferred, or terminally failed.

**branches**

- Recipient is suppressed → accepted-then-not-delivered, which must be a visible outcome and not a
  silent success (EV-011).
- Transient upstream failure → retried under a closed, documented retry-vs-terminal vocabulary; the
  DentConnex 429 defect is the scar this exists to prevent (EV-011).

**states**

- *error*: bad credential, unauthorized sender, malformed message — each distinguishable, because a
  consumer that cannot tell "retry" from "never retry" is the failure mode EV-011 names.
- *degraded*: the cluster is degraded but still accepting; acceptance must remain durable or be
  refused outright — never accepted-and-lost.

**packets**: PKT-11 (mail accept API + durable queue), PKT-13 (delivery workers + pacing + retry
classification), PKT-14 (suppression)

**open**: Which node sends — leader-only, or all nodes claiming from a replicated queue? What owns
SES's account-global pacing across three senders (OQ-3)? (data gate)

---

## F-07  Agent drives the full email loop against a test instance

```
actor:    agent
trigger:  a product's email-dependent flows need exercising
outcome:  the agent completed a real signup/verify/reset/support journey using a real inbox
surface:  machine-only
```

Named by the owner as a motivating case for the rebuild (EV-017).

**steps**

1. *(machine)* Agent works against an application registered in **testing** mode (F-05).
2. *(machine)* Agent starts a journey in the product — signs up as a persona.
3. *(machine)* The product sends via this service; the mail lands in Stalwart.
4. *(machine)* Agent reads the persona's inbox, extracts the verification link, and follows it
   against the product.
5. *(machine)* Agent continues: password reset, then emailing support and receiving a threaded reply.
6. *(machine)* Agent finishes with the persona verified and usable — the loop, not just one message.

**branches**

- Multiple personas in one run, distinguishable from each other and from other runs' mail.
- The product sends nothing when it should have → the agent must be able to tell "not sent" from
  "sent but not yet delivered", which is a question for this service, not for Stalwart.

**states**

- *empty*: no mail yet — normal, and must be distinguishable from failure so the agent waits instead
  of concluding.
- *error*: the message was accepted by this service but never reached the inbox — the single most
  important thing for this flow to make visible.

**packets**: PKT-13, PKT-15 (delivery observability / per-message state readable by machine)

**open**: Does this service provide the inbox-reading surface, or does the agent talk to Stalwart
directly (as today, via JMAP)? The evidence supports either; it changes this service's scope
materially. (data gate)

---

## F-08  Find out why mail did not arrive

```
actor:    owner (same surface serves the agent doing forensics — EV-015, EV-027)
trigger:  someone says they did not get an email
outcome:  the owner knows which of accept / deliver / suppress / bounce happened, and when
surface:  web + machine-only
```

*One of the two things the owner actually opens the UI for (EV-027): the "wtf is going on right now"
read. It must also exist as a machine-readable surface — agents run this same investigation, and
more often.*

**steps**

1. *(web)* Owner searches for the message — by recipient, by application, by time.
2. *(web)* Owner sees its actual lifecycle: accepted, attempted, and its terminal outcome, with the
   upstream's own answer where there was one.
3. *(web)* Owner sees whether the recipient is suppressed, and why.
4. *(web)* Owner acts, if there is an action — releasing a suppression, or re-sending.

**branches**

- Nothing found → the product never called this service, which is a different investigation and must
  be stated as such rather than shown as an empty result.

**states**

- *empty*: no matching message.
- *retained-metadata-only*: content is gone under retention while metadata remains (EV-011's
  metadata/payload split) — the UI must say the content was deleted by retention, not imply it never
  existed.

**packets**: PKT-15, PKT-16 (owner UI — message search and lifecycle), PKT-14

**open**: What is the retention default, and is it per application? (data gate)

---

## F-09  See that the fleet is healthy, and that edges agree

```
actor:    owner
trigger:  routine check, or something looks wrong
outcome:  owner knows the cluster's state and whether every edge is on the intended config
surface:  web + machine-only
```

EV-016 makes this the service's own most important self-report, and EV-027 makes it the other thing
the owner opens the UI for. Agents read the same facts to diagnose, so it is a machine surface too.

**steps**

1. *(web)* Owner opens the service and sees, without asking: the Raft cluster's members and leader,
   and each edge's last successful fetch and the config version it is on.
2. *(web)* Owner sees any edge that has not converged, and how long it has been that way.
3. *(web)* Owner sees whether mail delivery is flowing or backing up.

**branches**

- An edge is stale but still serving correctly (it has the previous good config) — a materially
  different situation from an edge that is down, and must not be shown the same way.

**states**

- *degraded*: quorum lost. The read surface must still answer, and must say plainly that writes are
  frozen while serving continues (EV-016 graceful degradation).
- *unknown*: an edge has not reported recently enough to claim anything about it — say "not heard
  from since X", never a green state inferred from silence.

**packets**: PKT-17 (convergence/health reporting), PKT-18 (owner UI — status)

**open**: Does this service also run the failover drill, or is the drill external (as with universe)?

---

## F-10  Rotate or revoke an application's credential

```
actor:    owner
trigger:  a credential leaked, or an application is decommissioned
outcome:  the old credential is dead; a live application is not accidentally silenced
surface:  web → outside-web
```

Credential custody is one of the two reasons this service is centralized (EV-014), so its lifecycle
is a journey, not a config detail.

**steps**

1. *(web)* Owner finds the application and sees when its credential was last used.
2. *(web)* Owner issues a replacement, with both valid for an overlap.
3. *(outside-web)* Owner or agent deploys the new credential into the application.
4. *(web)* Owner sees the application using the new credential, then revokes the old one.

**branches**

- Emergency revocation with no overlap — the owner accepts that the application stops sending, and
  the UI must state that consequence at the moment of the decision.

**states**

- *error*: revoking the last credential of an application that is actively sending — warn with what
  was actually observed (its last send), never a generic confirmation.

**packets**: PKT-10, PKT-12

**open**: Do routing-capable agent credentials follow the same lifecycle as application credentials
(OQ-2)?

---

## F-11  Bring an edge back after it has been unreachable

```
actor:    owner
trigger:  an edge was down, rebooted, or partitioned from the cluster
outcome:  the edge is serving the current config again, with no manual config editing
surface:  outside-web → web
```

Written because EV-016 makes graceful degradation and recovery an acceptance property, and because
this is where a distribution design either self-heals or does not (OQ-1).

**steps**

1. *(outside-web)* The edge comes back. It serves its last-known-good routes immediately, without
   reaching the control plane.
2. *(machine)* It re-contacts the service and takes whatever it missed.
3. *(web)* Owner sees it return to converged, and what it missed while away.

**branches**

- The control plane was the thing that was down, not the edge → edges kept serving throughout, and
  the owner should be able to confirm that no traffic was affected.

**states**

- *stale-but-serving*: the correct and expected state during the outage — the design's central claim,
  and the UI should be able to show it was true rather than assert it.
- *error*: the edge cannot re-authenticate after returning; must not be reported as "still
  converging".

**packets**: PKT-02, PKT-17

**open**: Blocked on OQ-1 — pull-based distribution (EV-026) gives step 1 and 2 natively; a push
design has to build both.

---

## F-12  See what configuration exists

```
actor:    owner
trigger:  curiosity, or the beginning of an investigation — "what do we actually have right now?"
outcome:  the owner has an accurate picture of the whole current configuration
surface:  web
```

Written because EV-027 names this explicitly as one of the few reasons the owner opens the UI at
all — wanting to see what configurations exist, or being curious what the routing configuration
looks like. It is not a sub-step of F-08 or F-09: those start from a symptom, and this starts from
nothing.

**steps**

1. *(web)* Owner opens the service and sees the whole picture without querying for it: every route,
   every edge, which routes each edge carries.
2. *(web)* Owner sees the same for mail: every registered application, its mode, and its sending
   volume (EV-027 names volumes specifically).
3. *(web)* Owner can follow anything that looks interesting into its detail — a route to its edges
   and its history, an application to its recent messages.

**branches**

- The investigation branch: this flow is where F-08 and F-09 often actually start, so it must lead
  into them rather than being a dead-end display.

**states**

- *empty*: nothing registered yet — should read as "nothing here yet", not as a broken page.
- *partial-truth*: some edges are unconverged, so what is configured and what is *being served* are
  not the same thing. This view must not conflate them; showing intent as though it were reality is
  the specific way this flow can lie.

**packets**: PKT-07, PKT-12, PKT-18, PKT-19 (mail volume reporting)

**open**: Is "volume" a counter this service witnessed directly (messages it accepted and delivered),
or an aggregate pulled from elsewhere? Per the owner's standing rule, a counter must name what the
system actually observed. (data gate)
