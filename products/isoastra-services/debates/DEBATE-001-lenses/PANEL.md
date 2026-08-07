# DEBATE-001 panel — isoastra-services T1

Derived per `debate-heuristics.md`. **No standing panel was reused.**

## Trigger

`debate: triggered (T3: key custody + identity model + API commitment; T2: no bound) → DEBATE-001`

T3 fires three times independently — the service holds SES credentials and issues every
application's M2M credential (EV-014, EV-023); the identity substrate is expected to move under it
(EV-015, OQ-2); and it commits a new mail API (EV-024). T2 fires because the bet carries no bound at
all (EV-019).

## Decisions this round serves

From the trigger and BRIEF-001's open questions: OQ-1 (edge config distribution), OQ-2 (identity
seam), OQ-3 (mail delivery across three nodes), OQ-4 (two-writer coexistence), OQ-5 (PHI enforcement
strength), OQ-6 (routing scope), OQ-7 (extensibility scaffolding), OQ-8 (certificates across edges).

**Explicitly not debated — already decided, recorded instead of argued** (heuristics step 3): the hub
thesis itself. The owner considered microservice separation and rejected it, naming the trade
(EV-012). No lens argues for splitting the product into separate services.

## Axes

| Axis | Ends | Why it is a real axis here |
| --- | --- | --- |
| A — blast radius | one-thing ↔ partitioned | EV-012 accepts coupling at the *product* level; whether routing and mailing share a failure domain at the *process/store* level is untouched by that ruling. |
| B — edge authority | live-now ↔ edge-autonomy | EV-026 found a native pull mechanism; EV-013's "agent deploys, route is live" wants immediacy. Both are owner values and they trade. |
| C — custody | one-vault ↔ single-breach | EV-014 makes centralized custody a stated benefit; T3 makes it the concentration risk. The trigger looking back at itself. |
| D — velocity | ship-it ↔ agent-did-it | Owner-requested. A solo founder working through agents wants minimum ceremony; agents acting without gates are the ones who can take everything down. |
| E — reliability | never-down ↔ you-maintain-this | Owner-requested. EV-016 makes reliability non-negotiable as a *value*; what it costs in mechanism, for one person to operate and debug, is not settled. |

Ten lenses, five axes, one agent per lens (heuristics: agent count is axis count doubled).

## Context asymmetry (rule 1, recorded)

**No lens receives `BRIEF-001`.** The brief is the interviewer's synthesis — the document that argues
the answer — and handing it to a lens leaks the author's framing into the round. Every lens receives
the raw evidence log, the flow addendum, and real code and procedure on disk. Grounding is by file
path only; no lens receives a prose summary of the bet.

Shared context, all lenses:

- `products/isoastra-services/evidence/` — EV-001..027, the raw record
- `products/isoastra-services/flows/FLOWS-001-routing-and-mailing.md`
- `~/dev/context/procedures/ha-service.md`, `~/dev/context/procedures/deployment.md`
- `~/dev/context/resources/networks.md`, `~/dev/context/resources/mail-fabric.md`
- prior art on disk: `priorart/service-ingress/` (the retired controller, its catalog and
  `docs/control-plane.md`), `priorart/mailer/` (the live mailer)
- `~/dev/personal/universe-ronitnath` — the reference hiqlite deployment
- `portfolio/products/ronitnath-universe/decisions/DEC-013-hiqlite-is-the-data-seam.md`

## Lens cards

### Axis A — blast radius inside one system

```
A1  one-thing
mandate:    It is 11pm and something is wrong. You are one person. You have to form a theory
            about what is broken, and every additional deployable, process boundary, store and
            interface between them is another place the theory can hide.
success:    A design where the number of things that can independently break is as small as it
            can be, and where one person can hold the whole failure surface at once.
forbidden:  You may not argue for separate deployables or separate stores. If isolation is the
            answer, you have conceded the axis.
```

```
A2  wedged-queue
mandate:    It is 3am. The mail delivery worker has wedged — a retry storm against SES has pinned
            the process — and the route-serving surface on the same node stopped answering with
            it. Public sites are down because an email would not send.
success:    A design where a failure in one service domain provably cannot take down another.
forbidden:  You may not argue on operational-simplicity or fewer-moving-parts grounds. That is
            the other lens's case and conceding it collapses the axis.
```

### Axis B — authority over the edges

```
B1  live-now
mandate:    You are an agent. The owner said "deploy this" four minutes ago and is watching the
            hostname in a browser. Everything is deployed. The route is registered. It is not
            serving yet, and you cannot tell him why or when it will.
success:    A design where a registered route is serving as immediately and as observably as the
            mechanism allows, and where "did it take" is answerable, not waited out.
forbidden:  You may not argue from control-plane-outage survivability. Assume the control plane
            is up; that is the other lens's ground.
```

```
B2  edge-autonomy
mandate:    The hiqlite cluster has been unreachable for forty minutes — a bad deploy, a
            partition, a quorum loss. Every public site must still be serving, and every edge
            that reboots during this window must come back serving.
success:    A design in which the data plane's correctness has no dependency on the control
            plane being reachable, and you can prove which parts keep working.
forbidden:  You may not argue from immediacy, propagation latency, or operator convenience.
```

### Axis C — custody versus concentration

```
C1  one-vault
mandate:    You have just found the same AWS SES credential pasted into five different service
            repositories, two of them with the secret in git history. Nobody knows which of them
            is still using it or how to rotate it without breaking something.
success:    A design where a provider credential exists in exactly one place, is rotatable
            without touching consumers, and where every send is attributable to a principal.
forbidden:  You may not argue that distributing credentials to consumers is safer. If dispersal
            is the answer, you have conceded the axis.
```

```
C2  single-breach
mandate:    One agent credential has leaked. With it, the holder can send mail as any brand the
            company owns and repoint every public hostname the company serves. Reconstruct what
            they can do before anyone notices, and how they would be noticed.
success:    A design where compromising one principal, or the service itself, has bounded and
            detectable consequences — and where the most dangerous capabilities are separated
            from the most-used ones.
forbidden:  You may not argue from convenience, from the small number of principals today, or
            from the owner being the only human. "It is just me and my agents" is the sentence
            that would let you agree with C1; you may not write it.
```

### Axis D — velocity for a solo founder working through agents

```
D1  ship-it
mandate:    You are the agent. The instruction was five words: "deploy this product." Everything
            you must supply that the system could have inferred, every precondition you must
            satisfy first, every confirmation you must round-trip to a human, is a step where
            this stalls or where you guess wrong and produce a subtly bad configuration.
success:    A design where the common path from "deployed" to "publicly serving" is the fewest
            possible required decisions, safe defaults do the rest, and an agent that gets it
            wrong is told exactly what to do instead.
forbidden:  You may not argue from safety, review, or the cost of a mistake. Any sentence of the
            form "but it should confirm before…" concedes the axis.
```

```
D2  agent-did-it
mandate:    An agent, acting on a five-word instruction and inferring the rest, registered a
            route that shadowed an existing hostname. Every site behind that edge served the
            wrong application for twenty minutes. The agent reported success throughout.
success:    A design where an autonomous principal cannot produce a broadly destructive outcome
            by inference or by a plausible mistake, and where the system's report of success
            means the thing is actually serving.
forbidden:  You may not argue from developer or agent convenience, or from how rare the mistake
            is. Rarity is the sentence that concedes the axis.
```

### Axis E — reliable services for a solo founder

```
E1  never-down
mandate:    A client product's mail has been silently dropping for six hours — accepted with an
            id, never delivered, nobody told. In the same window a route change froze because the
            cluster lost quorum, and nothing said so. The owner found out from a customer.
success:    A design where every failure is either impossible, self-correcting, or loudly
            visible with the system naming what it actually observed — and where degraded modes
            are designed rather than discovered.
forbidden:  You may not argue from implementation cost, scope, or "that is a later phase."
```

```
E2  you-maintain-this
mandate:    It is 3am and the thing that is broken is this service itself. You are alone. You
            last read this code four months ago. Every reliability mechanism in it — the quorum,
            the retry classifier, the convergence tracker, the versioning, the drill harness —
            is code that can itself be the bug, and tonight one of them is.
success:    A design small enough that one person can hold it, where each mechanism earns its
            existence against the failure it prevents, and where the service's own failure modes
            are fewer than the ones it protects against.
forbidden:  You may not argue that a reliability mechanism is inherently worth its complexity,
            and you may not propose adding a mechanism to monitor another mechanism.
```

## Round provenance (rule 7)

- Orchestrator/judge: Claude (Opus 5). **The judge authored the flow addendum and the brief — the
  judge is compromised (rule 6), and this aggregation is itself an owner input, not a verdict.**
- Lenses: `gpt-5.6-sol` via `codex exec`, one detached OS process per lens, launched with file paths
  only (rule 5 decorrelation: different model family from the orchestrator; `procedures/orchestration`
  routing policy names this tier for adversarial review).
- No lens received `BRIEF-001`. No lens was told the round's shape, the other lenses' names, or the
  axis it sits on.
