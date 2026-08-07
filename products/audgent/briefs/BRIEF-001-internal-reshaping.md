---
product: audgent
bet: 1 — internal reshaping (phonetics split out as bet 2, EV-022)
date: 2026-08-06
interviewer: claude (opus-5), PM-mode session
status: final — amended 2026-08-06 after flow review (EV-024..EV-028)
confidence: medium
---

> **Amendment, 2026-08-06.** Reviewing FLOWS-001 produced five further rulings, two of which
> change the bet rather than clarify it: **wire-level request/response inspection** (EV-026) is
> a new fifth scope item and the largest data commitment in the bet, and **agent diagnosis**
> (EV-028) is admitted as a fifth agent verb after being written as a non-goal. Also: the owner
> retains direct edit capability (EV-024, qualifying EV-012), provider selection is what
> "configuration" chiefly means (EV-025), and the existing console surfaces stay and are re-cut
> for one org rather than replaced (EV-027, resolving OQ-5). Sections below are updated in place;
> this note records that the interview did not surface EV-026 and the flow review did.

# audgent — bet 1 brief: internal reshaping

audgent is a fork of dograh, a self-hostable vapi/retell alternative. It was built for a
multi-tenant world it will never be in. This bet takes that shape off it.

## Agenda coverage

| Topic | State | Distillation |
| --- | --- | --- |
| Problem | covered | A multi-tenant console structurally shows the owner one workspace when he needs the whole system (EV-010); the SaaS shape is the defect, not any feature (EV-001) |
| Users | covered | One human (the owner — inspecting mostly, editing rarely) and worker agents (authoring, testing, diagnosing); one consuming product, rinity (EV-009, EV-011, EV-024, EV-028) |
| Bound | **waived** | No deadline, no session budget — an explicit owner waiver of SYS-DEC-001 (EV-020) |
| Constraints | covered | Almost none: nothing is carrying calls, no dates, no provider is load-bearing (EV-019) |
| No-gos | covered | Everything not discussed in this interview (EV-021); phonetics, deferred to bet 2 (EV-022) |
| Success signals | covered | See below — derived from EV-010/EV-013/EV-014 |
| Risks / irreversibility | covered | Long horizon, not a short stopgap (EV-023); tenancy teardown is a schema commitment |

## Answers

### Problem — the tenant model hides the system

audgent's inherited design assumes a customer operating their own slice. The owner is not a
customer, and being modelled as one is what puts a wall between him and his own system: he
sees his workspace and his workflows, not what is going on (EV-010). This is the same
complaint he has about Stalwart's admin surface, so it is a pattern he recognizes, not a
one-off irritation. The generic-product shape is what is being discarded (EV-001); individual
features are only guilty by association with it.

Two live pains sit on top of that. Configuration is built as a BYOK self-serve form flow for a
human who arrives with vendor keys — but the keys are already the owner's and the filling-in
is agent work (EV-012). And cost is unattributable: agent test traffic and production traffic
land in the same undifferentiated spend, so the owner can neither explain it nor control it
(EV-013).

### Users — one inspector, several workers, one consumer

- **The owner**, predominantly inspecting. His surface is five reads: calls going through,
  call logs, costs, current configuration, and what the agents have been changing (EV-014) —
  plus, below all of them, the wire (EV-026). He retains direct edit capability and uses it
  rarely (EV-024): agents are the normal path, not the only one. What "configuration" mostly
  means is which providers to use (EV-025).
- **Worker agents**, the primary interfacing surface (EV-006), with five verbs: create
  workflow, test workflow, configure provider, test provider (EV-011), and diagnose a failure
  (EV-028). The *test* verbs let an agent close its own loop without a human confirming that a
  change worked; the *diagnose* verb lets it close the loop when the answer is bad.
- **rinity** (the front desk service), the only consuming product, plugging in downward with
  its own auth key (EV-009, EV-017). `frontdesk-dental` and its predecessors are inactive and
  are not callers.

### Position — interim in role, indefinite in duration

audgent is the production surface while the greenfield `voice` product is built (EV-007), and
it is disposed of only when `voice` is hardened and production-tested — which the owner
explicitly says not to assume is near (EV-023). **This inverts the appetite argument.** A
short-stopgap reading would justify cheap throwaway work; the actual horizon means cost of
ownership outweighs cost of construction. Build it to stay legible, inside the discussed scope.

The codebase continues, modified in place — never rebuilt, because rebuilding is `voice`'s job
(EV-018).

### Scope — five changes

1. **Degenerate to one organization** (EV-017). Tenancy is never needed: products are the unit
   that gets a credential, not customers, and rinity holds whatever per-customer separation its
   own market requires above this line. Per-org scoping, quotas, and concurrency slots stop
   being product surface. This is the change that unblocks (2).
2. **Whole-system observability** (EV-010, EV-014). The five reads, each answerable across the
   system rather than within a workspace.
3. **Agent-first authoring and testing** (EV-011, EV-012). The five verbs as a first-class
   surface; configuration screens become read-first, with the owner's rare direct edit
   preserved rather than removed (EV-024, EV-027).
4. **Cost attribution** (EV-013). Every run labelled production or agent-test, with cost
   rolling up separately. Two distinct demands live here — attribution (can I see it) and
   containment (is the cheap path the default for agent testing) — and they should not be
   conflated in packets.
5. **Wire-level diagnostics** (EV-026, EV-028). Every outbound request and response — to
   providers and to tool endpoints — captured, ordered, and reachable from the run: by the
   owner as a drill-down (F-10) and by an agent as a query (F-11). The failure it targets is
   *silent wrongness*, not error pages: a tool call rejected on its schema leaves a transcript
   in which the agent simply, inexplicably, didn't do the thing. This was not surfaced in the
   interview and is the largest addition to the bet — it is a volume, retention and redaction
   commitment, since captured payloads are big and may carry credentials or PHI.

Out: phonetic misrecognition repair (bet 2, EV-022), and everything not discussed — local
inference depth, the immutable deployment contract, campaigns, provider parity, the
workflow-graph model itself (EV-021).

### Constraints — unusually few

Nothing is carrying calls, so no provider is untouchable, no migration needs a zero-downtime
story, and there is no traffic to break (EV-019). No dates. The planned work does not sit in
the call path regardless. This is what makes a schema-level tenancy teardown affordable.

### Bound — waived, explicitly

The owner declined both denominations (EV-020). The consequence is named rather than absorbed:
the bound is what makes scope flex instead of grow, and nothing will fire a "budget exhausted"
stop. **Scope control for this bet rests entirely on EV-021** — an addition that wasn't
discussed is out by default and needs a new evidence item to get in.

### Success signals

- The owner opens one surface and sees every call, log, cost, and configuration in the system —
  no workspace switching, nothing scoped away (EV-010).
- Spend splits cleanly into production and agent-test, and the split is readable without
  reconstruction (EV-013).
- An agent creates a workflow, tests it, configures a provider, tests it, and reads back a
  verdict — with no human in the loop and no console visit (EV-011).
- The owner can answer "what have the agents changed" from the interface (EV-014).
- A call fails, and the owner names the cause from the request and response themselves — not
  by inference from a transcript — in one drill-down from the call (EV-026). The sharper
  version: the vapi case, where the call *sounded* fine and a tool call was being rejected on
  its schema, is diagnosable here in minutes.

### Risks and irreversibility

- **Tenancy teardown is a schema commitment** on stored data and touches the auth/caller model.
  That is a T3 debate trigger by the letter of `debate-triggers.md`, and it is the one
  genuinely expensive-to-reverse move in the bet. Mitigated but not erased by EV-019 (no live
  traffic to migrate).
- **No bound + no clock** (EV-020, EV-023) is the drift risk. The scope list above is the
  guard; treat it as a fence, not a starting point.
- **Debug logging is already leaking payloads** (EV-029): `LOG_LEVEL` defaults to `DEBUG`, so tool
  request bodies flow to container logs today with no retention or redaction policy. Small to fix,
  and it should not survive a bet that is explicitly deciding a payload-capture posture.
- **The agent-test cost demand can regress under its own solution**: giving agents a test verb
  (EV-011) increases exactly the traffic EV-013 complains about. Attribution must land with the
  verb, not after it.

## Open questions

| OQ | Statement | Why it matters | Blocking? |
| --- | --- | --- | --- |
| OQ-1 | Does "one organization" mean removing org scoping from the schema, or pinning a single org row and hiding it? | **Sized by EV-029**: 41 org columns, 202 files, 98 migrations — wide and mechanical, not deep. Degeneration is far cheaper and reversible; removal buys a smaller schema and no vestigial concept. EV-019 makes either affordable, EV-023 argues for doing it properly | no — but this is the decision the debate should take |
| OQ-2 | What replaces per-org identity as the caller model — existing Kanidm service principals and scopes, or something simpler now that there is one org and two callers? | The current machinery was built for customer backend integrations; whether it fits product keys and agent callers is untested | no — data gate |
| OQ-3 | What does an agent's "test" verb actually return? Is the T9 harness / call-flow evals the substrate, or is this a new agent-facing capability? | The two test verbs are the load-bearing half of EV-011; without a verdict contract they are just a trigger | no — but blocks packet derivation |
| OQ-4 | Is production-vs-agent-test a property of the credential, of the run, or declared per call? | Credential-derived is self-enforcing and needs no discipline; declared is trivially wrong the first time an agent forgets | no — data gate |
| ~~OQ-5~~ | ~~Does the human surface stay the existing console reshaped, or become a new surface?~~ | **Resolved by EV-027**: the surfaces stay — flow builder, model configuration and the rest are re-cut for one org. Only three views are new construction: agent activity, the cost split, and the wire drill-down | resolved |
| OQ-8 | Retention and redaction for captured request/response payloads — how long, what is scrubbed, are agent-test and production runs treated alike, and is capture always-on or armed? | Always-on capture is what catches the failure nobody predicted, and is also what makes volume, cost and PHI exposure real (EV-026). **Scoped by EV-029**: the tool *response* half largely exists (status code + parsed body persist in run logs and already render); the tool *request* half goes only to the Python logger; provider exchanges aren't captured at all unless Langfuse is enabled, and it defaults off | no — data gate, but it is the bet's largest new commitment |
| OQ-10 | `LOG_LEVEL` defaults to `DEBUG`, so tool request bodies already flow to container logs — uncorrelated to runs, with no retention or redaction policy (EV-029) | The payloads OQ-8 proposes to capture deliberately are partly being captured accidentally today, with none of the controls. It is also cheap to fix | no — but answer it in the same breath as OQ-8 |
| OQ-9 | With editing retained but rare (EV-024), how does the configuration surface stay read-shaped without hiding the edit path? | The inherited console gets this exactly backwards — it is built for data entry — and re-cutting it is most of the UI work | no — design gate |
| ~~OQ-6~~ | ~~Does a configuration change history exist today, or must it be introduced?~~ | **Answered by EV-029: it does not.** No audit table, no `updated_by` column anywhere. Workflows are the sole exception — genuinely versioned, with runs pinning their definition. Provider and model configuration is overwritten in place. So F-5 is new construction over exactly the space EV-025 calls the main one, and **it must land before agents start changing things** — history cannot be backfilled | answered; the commitment stands |
| OQ-7 | Is `voice` allowed to inherit anything built here, or is the split absolute? | Bears on whether seams are worth generalizing; EV-016 already assumes the pipecat work carries forward for bet 2 | no |

None blocking. All inherited by the debate or sanity pass. The source check (EV-029) closed OQ-6 and
sized OQ-1 and OQ-8; what remains is a **two-part data commitment**, not three: a configuration
change log (none exists) and wire capture (half exists, and it is the wrong half — responses without
requests). Both must be settled before agents start changing things, since neither can be backfilled.

## Debate recommendation

`debate: triggered (T3: tenancy teardown is a schema + auth-model commitment)` — recommended,
not decided; the pitch records the line. Note that **T2 cannot be evaluated** because the bound
is waived (EV-020), and T1 does not fire (tier `real`, and audgent is pure internal — the
customer-facing plane is rinity). Confidence is `medium` rather than `high`: the direction is
dense ground truth with no evidence conflicts, but four of the seven open questions are
data-model shaped, and the bet's one irreversible move sits exactly there.
