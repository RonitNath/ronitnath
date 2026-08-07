---
product: audgent
pitch: PITCH-001 — take the SaaS shape off the engine (bet 1)
date: 2026-08-06
status: DRAFT — not frozen. Debate runs before freeze; freeze tags `pitch/audgent-1` and FLOWS-001 with it
sources: BRIEF-001 (amended), FLOWS-001 rev 2, EV-001..EV-030, DEC-001, EV-029 (source check)
debate: triggered (T3: a new configuration change log + wire-payload capture are commitments on stored data, and the caller/auth model changes from tenant memberships to product keys) → memo DEBATE-001. Note T2 is unevaluable — bound waived (EV-020)
---

## Problem

audgent is a dograh fork, and dograh is a self-hostable vapi/retell alternative: a product
built to be operated by strangers, for workloads its authors never see. Every consequence of
that shape is wrong here. The owner is modelled as a tenant, so the console shows him his
workspace instead of his system (EV-010). Configuration is a BYOK form flow for a customer
arriving with their own vendor keys, when the keys are already his and the filling-in is
agent work (EV-012). Agent test traffic and production traffic land in the same
undifferentiated spend, so cost can be neither explained nor controlled (EV-013). And when a
call goes wrong, the evidence stops at the transcript — the request that was rejected and the
response that rejected it are not in the run (EV-026, EV-029 §2).

The last one is the sharpest, because it is the failure that doesn't look like a failure: a
tool call refused on its schema produces a call that *sounds fine* and work that never
happened. The owner has diagnosed exactly this before, on vapi, by reading the wire (EV-026).

This is the production surface under the front desk service while `voice` is built (EV-007),
and it is disposed of only when `voice` is hardened and production-tested — explicitly not
soon (EV-023). It is interim in role, indefinite in duration. Build for ownership, not
for disposal.

## Bound

**Waived** (EV-020: "No dates." / "No bounds."). This is a deliberate departure from
SYS-DEC-001 and the cost is named rather than absorbed: nothing will fire a budget-exhausted
stop, so scope cannot flex against a budget. **Scope control rests entirely on EV-021** —
"just stick to what we've discussed." An addition that was not discussed is out by default
and needs a new evidence item to get in, not a judgment call. The five changes below are a
fence, not a starting point.

Standing offer, unexercised: name a review-session budget at any point and the bet acquires
a stop.

## Rough solution — five changes

**1. Degenerate to one organization** (DEC-001, EV-017, EV-030). Exactly one org; its id is a
constant, not a choice. No switcher, no selector, no creation path, no org shown as context.
Reads never scope — a view that filters by org is a bug, since whole-system visibility is the
entire point. Product-level keys (rinity's, the agents') resolve to the single org implicitly.
Columns stay; per-org limits become global limits. This is a behavioral change, not a
migration — that is what makes it the first move.

**2. Whole-system observability** (EV-010, EV-014). Five owner reads, each answerable across
the system: calls in flight, call logs, cost, current configuration, and what the agents have
been changing. The first four re-cut what exists (EV-027); the fifth and the cost split are
new.

**3. Agent-first authoring, testing and diagnosis** (EV-011, EV-028). Five verbs as a
first-class surface — create workflow, test workflow, configure provider, test provider,
diagnose a failure. The test verbs are the load-bearing half: they let an agent close its own
loop without a human confirming a change worked. The owner keeps direct editing, used rarely
but genuinely (EV-024); the existing flow builder and model configuration screens survive,
re-cut for one org (EV-027), read-first rather than entry-first.

**4. Cost attribution** (EV-013). Every run labelled production or agent-test, with cost
rolling up separately, and an explicit unattributed bucket rather than a silent default. Two
demands, not one: attribution (can I see it) and containment (is the cheap path the default
for agent testing). Attribution must land *with* the test verbs, since giving agents a test
verb increases exactly the traffic this complains about.

**5. Wire-level diagnostics** (EV-026, EV-028). Every outbound request and response — to
providers and to tool endpoints — captured, ordered, and reachable from the run: rendered for
the owner (F-10) and queryable by an agent (F-11). One record, two consumers; the agent path
constrains the shape more, so it leads.

### What the source check changed (EV-029)

Two of these are less finished than they look, in opposite directions:

- **The configuration change log does not exist.** No audit table, no `updated_by` column
  anywhere. Workflows are the sole exception — genuinely versioned, runs pinning their
  definition. Provider and model configuration is overwritten in place, which is precisely the
  space EV-025 calls the main one. **This must land before agents start changing things**;
  history cannot be backfilled.
- **Wire capture is half-present, and it is the wrong half.** Tool *responses* already persist
  into `workflow_runs.logs` with status code and parsed body, and already render. Tool
  *requests* go only to the Python logger. Provider exchanges are not captured at all unless
  Langfuse is enabled, and it defaults off. The missing half is the one the owner named.

## Flows

Full addendum: [`FLOWS-001-internal-reshaping.md`](../flows/FLOWS-001-internal-reshaping.md)
(rev 2). Freezes under the same tag.

| ID | Actor | Outcome |
| --- | --- | --- |
| F-1 | OWNER | Sees every call in flight across the system; empty is the normal case and must not read as broken |
| F-2 | OWNER | Reconstructs what an agent said and did on a past call, and reaches the configuration it ran under |
| F-3 | OWNER | Says how much was production and how much was agents testing, without reconstructing it by hand |
| F-4 | OWNER | Reads the live configuration — above all which providers are in use — and can edit it directly if he chooses |
| F-5 | OWNER | Sees the sequence of configuration changes agents made, with enough context to judge each |
| F-6 | OWNER + AGENT | Hands a fix to an agent instead of making it, and confirms it landed |
| F-7 | AGENT | Builds a workflow and proves it works from a verdict, with no human confirming |
| F-8 | AGENT | Configures a provider and proves it works, with no human touching a form |
| F-9 | RINITY | Runs production traffic under its own product key, attributed as production |
| F-10 | OWNER | Drills into the wire to find out why something failed — request, response, order |
| F-11 | AGENT | Diagnoses a failure from the same record, by query rather than by render |

## Rabbit holes

- **Removing org scoping instead of degenerating it.** Ruled out in DEC-001. 41 columns, 202
  files, 98 migrations for a benefit no flow depends on.
- **Rebuilding the console.** EV-027 and EV-018 both forbid it. The surfaces stay and get
  re-cut. Only three views are new construction: agent activity, the cost split, the wire
  drill-down.
- **Always-on payload capture without deciding retention and redaction first.** Always-on is
  what catches the failure nobody predicted, and is also what makes volume, cost and PHI
  exposure real. This is a data-gate decision, not an implementation detail (OQ-8).
- **Building the wire record for the UI and adding an API later.** The predictable mistake.
  An agent needs to query the record, not receive it; that constraint should shape it first.
- **Provider parity.** The temptation while working in provider configuration is to level up
  the long tail of vendors. Out (EV-021). Provider *selection* is the configuration space
  (EV-025); provider *breadth* is not a goal, and EV-003 suggests it is closer to an obstacle.
- **Anything that improves the engine generally.** `voice` is the durable answer (EV-007).
  Improvements that are not on this list belong there.

## No-gos

- **Phonetic misrecognition repair** — bet 2 (EV-022, EV-015, EV-016). Must not leak into this
  pitch, its packets, or its gates.
- **Anything not discussed** (EV-021): local inference depth, the immutable deployment
  contract, campaigns, the workflow-graph model itself.
- **A customer- or tenant-facing surface.** Nobody outside signs in to audgent. rinity is the
  external plane and holds whatever per-customer separation its market needs (EV-017,
  rinity/EV-015).
- **A self-serve configuration journey.** The owner edits (EV-024) and the surfaces survive
  (EV-027), but nothing is shaped for a stranger arriving with their own keys.
- **Rebuilding.** In-place modification only (EV-018).

## Open questions carried into the debate

From BRIEF-001, minus those since answered: OQ-2 (what replaces per-org identity as the caller
model), OQ-3 (what an agent's test verb actually returns — the verdict contract), OQ-4 (is
production-vs-agent-test a property of the credential, the run, or declared), OQ-7 (may `voice`
inherit anything built here), OQ-8 (retention/redaction/always-on for captured payloads),
OQ-9 (how a read-shaped surface keeps a rare edit path available), OQ-10 (`LOG_LEVEL=DEBUG`
already leaks tool request bodies to container logs, uncorrelated and unredacted — cheap to
fix, and it should not survive a bet that is deciding a capture posture).

OQ-1 and OQ-6 are answered (DEC-001; EV-029 §1). OQ-5 is resolved (EV-027).
