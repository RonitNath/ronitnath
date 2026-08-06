# Spec: the heuristic panel — how a debate gets real disagreement

A debate between agents fails by default, because agents spawned from the same model with the same
context produce the same opinion in different voices. Telling them to "be adversarial" doesn't fix
it — they perform disagreement and converge anyway. The fix is **structural**: give each agent a
different *job with a different success criterion*, so disagreement is a consequence of the mandate
rather than a request.

Lenses come in **opposed pairs**. Each pair is an axis the product genuinely sits on, and neither end
is correct — the value is the tension, and where two lenses collide is where a real decision is
hiding. A round that produces no collisions was run wrong.

## The panel

### Axis 1 — Time: cost now against cost later

| | **H1 Minimalist** | **H2 Archivist** |
| --- | --- | --- |
| Mandate | Find the cheapest thing that satisfies the evidence log. | Judge every decision by what survives when the code is gone. |
| Success | Fewest tables, surfaces, packets, concepts. | In 2031 the database alone is self-describing and lossless. |
| Forbidden | Arguing for anything on grounds of future need. | Arguing for anything on grounds of build cost. |

### Axis 2 — Whose experience wins: owner against guest

| | **H3 Operator-under-load** | **H4 Guest-dignity** |
| --- | --- | --- |
| Mandate | It's 9pm at the event. Phone, one hand, bad wifi, people talking to you. | You are the person who got a link and never agreed to be in anyone's system. |
| Success | Every owner task completes without reading anything. | Nothing surveillance-shaped, nothing confusing; forwarding a link isn't a trap. |
| Forbidden | Caring what the guest sees. | Caring whether the owner's job gets harder. |

### Axis 3 — Variance: platform against bespoke

| | **H5 Cohesion** | **H6 Expressive-variance** |
| --- | --- | --- |
| Mandate | With per-event code, name what stops this becoming N unrelated websites. | The bet exists because events must be able to differ wildly (EV-011). |
| Success | A new event inherits nearly everything; bespoke work is small. | An event can look and behave like nothing else without fighting the system. |
| Forbidden | Accepting "the agent writes it per event" as an answer. | Accepting "put it in the shared library" as an answer. |

### Axis 4 — Control: autonomy against recoverability

| | **H7 Agent-autonomy** | **H8 Failure-realist** |
| --- | --- | --- |
| Mandate | The creating actor is a machine that never sees the UI. | Assume concurrency and partial failure everywhere. |
| Success | An agent creates a whole event unattended from the API and can diagnose failure from a transcript. | No unrecoverable state; every partial failure has a defined resolution. |
| Forbidden | Proposing anything that needs a human to look at a screen. | Proposing anything that trades correctness for convenience. |

## Rules of the round

1. **Asymmetric context, recorded.** Lenses whose job is to attack a conclusion do **not** receive the
   document that states it — H1, H2, H4 and H8 get the evidence log and the code repo, not the pitch.
   Which context a lens held is part of its claims' provenance.
2. **Grounding beats reasoning.** Lenses get repo and filesystem access. A claim that cites an
   artifact — a schema, an old codebase, a real seed file — outranks one that reasons in the abstract.
   **A round whose claims are entirely `inference` is void** (extends the T6 trigger inward).
3. **The null result is expensive.** Every lens returns at least one claim it would stake the bet on.
   The judge records, by name, why each rejected claim was rejected. "No fatal flaws" must cost
   something to write.
4. **Collisions are the output.** Where two lenses contradict, the pair is recorded as a `rebuts` edge
   and resolved explicitly — resolved / deferred-as-assumption / unresolved / escalated. An axis with
   no collision means one of its lenses didn't do its job.
5. **Different model family where the stakes justify it.** The largest decorrelation available, and it
   costs a parameter. Recorded on the debate.
6. **The judge is compromised.** Whoever wrote the artifact under debate cannot be the sole
   adjudicator of it; their aggregation is itself an input the owner reviews. This is a known,
   unfixed limitation — stated so it isn't mistaken for rigour.

## Scaling

Not every debate runs eight lenses. Pick the axes the decision actually sits on: a schema commitment
runs Axis 1 and 4; a UI direction runs Axis 2 and 3. Running an axis the decision doesn't touch
produces polite agreement, which is worse than not running it — it looks like corroboration.
