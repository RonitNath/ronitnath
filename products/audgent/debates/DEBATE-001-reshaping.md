# DEBATE-001 — audgent bet 1, internal reshaping

Generated from `DEBATE-001-reshaping.yaml`. 4 axes, 8 lenses, 63 claims, 0 inference-only
claims, every lens staked one. Lens runtime: `gpt-5.6-sol` via pi, eight detached processes on
nexus. No lens received the pitch or the brief.

**The judge is compromised** (heuristics rule 6): I wrote BRIEF-001, FLOWS-001 and PITCH-001,
and I designed the panel that decides what could be found. This memo is an input for the
owner, not a verdict. Three claims I verified myself in the source before writing them up,
because they change the bet and I did not want to relay them on an agent's word alone.

---

## What the round found that the bet did not know

Three defects, all confirmed by the judge independently. None was in the interview, the
flows, or my source check.

### 1. The engine reports failed tool calls to the voice agent as successes

`api/services/workflow/tools/custom_tool.py:353` sets `"status": "success"` for **any** HTTP
response it receives. A 404, a 422 schema rejection, a 500 — all return to the LLM as a
successful tool result, with the error body tucked inside `data`. Only transport failures
(timeout, connection error) produce `status: "error"`.

This is the vapi failure mode (EV-026), present in the code, right now. It is the mechanism
by which a call sounds fine and the work never happened: the model is told its booking
succeeded. A1-2 found it; C1-5 and A1-3 sit next to it.

**It also reframes scope item 5.** Wire capture would let you *diagnose* this after the fact.
Fixing the classification stops it happening. Those are different pieces of work and the
pitch contains only the first.

### 2. "Test provider" is currently a lie for most providers

`api/services/configuration/check_validity.py:313-334` — the validators for ElevenLabs,
Google, Azure (three variants), Cartesia, Sarvam and OpenRouter `return True` unconditionally,
without a network call. Groq and Inworld do real checks.

EV-011 names *test provider* as one of the agent's five verbs, and F-8 requires the agent to
"exercise it — the smallest real call that proves the credential and the path." The existing
implementation would answer "working" for a provider with a garbage key. C1-5 found it;
D1-6 and C1-4 add that those endpoints reject service principals anyway, so an agent cannot
reach them today regardless.

### 3. There is no cost accounting to attribute

B1-4 claims, and I confirmed, that nothing writes `cost_info` on the pipeline path:
`format_public_cost_info` documents itself as returning "the legacy response shape **without
doing local cost accounting**," and no caller passes `cost_info` into
`workflow_run_client.update`. Usage *quantities* are aggregated; cost is not computed. The
hosted model-proxy that used to price runs was removed in the fork (EV-011).

**EV-013 is the bet's most concrete owner pain — "there's a lot of cost" — and scope item 4
assumes there are cost numbers to split.** There are not. The work is *compute cost, then
attribute it*, which is materially larger than labelling runs.

---

## Collisions and how they resolved

### The strongest result: five lenses, one line of code

**B2-5 is rejected.** B2 argued that no new run-provenance field is needed, because public
runs already carry `trigger_mode` and service-principal identity in `initial_context`. Five
lenses on three different axes — D1-1, D2-3, C1-7, C2-5, B1-3 — independently found that
`initial_context` is caller-writable at `api/routes/public_agent.py:229-250`, and that a
caller can overwrite both the trigger mode and the principal attribution before insertion.

A forgeable field cannot carry the production-vs-agent-test split. That five lenses with
opposed mandates converged on the same file and line is the strongest signal the round
produced.

### Axis D was not the disagreement it looked like

D1 (the credential decides) and D2 (the run carries its own truth) largely agree, and the
synthesis takes both: **cost class and actor are derived from the credential** — unforgeable,
zero discipline at the call site (D1-2) — **and stamped immutably onto the run at creation**,
in non-client-writable columns, so they survive key rotation, sharing and reuse (D2-1).

Three riders survive unopposed:
- **Purpose is distinct from cost class** (D2-4): workflow-test, provider-test and production
  are three purposes; the first two share one cost class.
- **Non-optional in the central run constructor** (D2-6), or the ingress paths that don't set
  it keep minting unattributed runs.
- **Set before the first provider operation** (D2-8), not at completion, or calls that die
  mid-pipeline lose their provenance — which is exactly the population you most want to
  attribute.

D2-5 sharpens why this matters: there are **eight run-creation sites**, and only the
public-agent path snapshots a service principal. The others record incompatible fragments or
no actor at all.

### Axis A split rather than collapsed

A1 (wire-or-nothing) and A2 (least-data-kept) collided on capture scope, and the resolution
divides the record in two:

- **Envelope metadata is always-on** — method, URL, status, timing, monotonic sequence,
  correlation id, an explicit *send-intent* record written before egress, and a terminal
  completeness marker. A1-6's point is structural and unanswerable: a capture holding only
  completed request/response pairs cannot prove that **no request was sent**, which is one of
  the four cases you must distinguish at 2am.
- **Bodies are governed by policy** — they are where the volume and the PHI live. OQ-8 must
  set retention, redaction and scope before this is built.
- **B2-7 wins unopposed**: capture only the boundaries rinity and agent tests actually
  exercise, not the inherited provider catalog.

A2 also produced findings that stand entirely on their own, outside the axis:

- **A2-2**: the DEBUG default writes complete tool arguments and request bodies to stdout, and
  the application's seven-day retention bound **does not apply to that stream**. This confirms
  OQ-10 and makes it worse than I recorded it.
- **A2-4**: reading an artifact-bearing run mints a **public download token with no expiry
  column and no revocation check** (`api/routes/public_download.py:50-98`) — so a token
  obtained through a later-revoked credential keeps working against that run's recordings and
  transcript. Nobody asked for this; it is a live exposure, not a design question.
- **A2-5**: a normal recorded call persists mixed, user and bot audio, a transcript file, *and*
  transcript text inside the run events — five copies where F-2 names two.

### Rejected, with reasons

| Claim | Why rejected |
| --- | --- |
| **B2-5** — `initial_context` already carries provenance | Caller-writable and overwritable; five independent lenses, same file:line |
| **B2-2** — pinning one org makes existing scoped reads whole-system | Necessary, not sufficient. B1-1: the app can still create organizations. C2-7: a mis-scoped read drops rows **silently**, which is precisely the risk DEC-001 knowingly accepted. Becomes two build rules — disable every org-creation path, and audit reads for `selected_organization_id` |
| **B2-6** — activity history as JSON under a reserved config key, no migration | Cannot serve F-5's time-ordered whole-system read with per-principal attribution (C2-2, D1-5), and misses workflow drafts, which mutate in place (C2-4) |

### Deferred as assumption

**C2-3 vs A2-7** — how rich the activity record is. Assumed: actor, object, timestamp, changed
field, redacted before/after, plus a free-text rationale the agent supplies. No payloads, no
credential values. Cheap to reverse either way; revisit if a six-month reconstruction actually
fails.

---

## What survives, as build rules

1. Disable every organization-creation path; audit owner-facing reads for `selected_organization_id` filters (B1-1, B1-2, C2-7, B2-2).
2. Cost class + actor derived from the credential, stamped immutably on the run at creation, non-client-writable; purpose recorded separately; required in the central constructor; set before the first provider operation (D1-2, D2-1, D2-4, D2-6, D2-8).
3. Each worker agent gets its own credential — two agents under the owner's MCP session are indistinguishable in mutation history today (C2-2, D1-5).
4. The configuration change log is a real append-only table with per-principal attribution, not a JSON blob; it covers provider/model config, tools, credentials **and** workflow drafts (C2-1, C2-4, B1-5, B2-6-rejected).
5. Wire record: envelope metadata always-on with send-intent and completeness marker; bodies under an OQ-8 policy; only boundaries rinity and agent tests exercise (A1-6, A2-3, B2-7).
6. Provider config and validation endpoints must accept service principals, not only operator sessions (C1-4, D1-6, C1-3).
7. A test verb needs an expectation and a verdict; today there is neither (C1-2).
8. Runs must pin the resolved provider/tool/credential values they ran under, not only the workflow definition (A1-5, C1-8).

---

## Escalated to the owner

Batched, per `debate-triggers.md` — one round, not a drip.

**E-1 — Are the three discovered defects in scope?** The tool-call success misclassification,
the stub provider validators, and the absent cost accounting are all *within* the areas the
bet already touches, but none was discussed, and EV-021 puts undiscussed work out by default.
Two of them make stated scope items impossible as written: item 4 cannot split costs that are
never computed, and item 3's "test provider" verb would return a validated lie. B2-1 argues
the fence should hold anyway; the fence cannot decide its own exceptions.

**E-2 — What is the retention, redaction and capture posture?** (OQ-8, unchanged by the round
but now sharper.) Bodies are where cost and PHI live. Always-on capture is what catches the
failure nobody predicted; it is also what creates the liability. A2-2 and A2-4 show the
system is already leaking in both directions with no policy at all, so "decide later" is not
the neutral option it sounds like.

**E-3 — Two live exposures, independent of this bet.** Public download tokens with no expiry
or revocation (A2-4), and DEBUG-level logging of tool request bodies to an unretained stream
(A2-2). Both are cheap to fix and neither needs a bet. They want a yes/no now rather than a
place in a pitch.

---

## What would change the recommendation

- If cost accounting turns out to exist somewhere outside the pipeline path I checked, scope item 4 shrinks back to labelling and E-1 loses a third of its weight.
- If the owner rules that agent-test runs are never worth capturing bodies for, the A-axis split collapses toward A2 and the retention question gets much easier.
- If a second consuming product appears (EV-009 says one exists today), D1's credential-classing needs a third class and DEC-001's degeneration reopens.

## What this round could not see

The panel I designed decides what is findable, and I also wrote the artifacts under attack.
No lens was pointed at the *flows themselves* — whether F-1..F-11 are the right journeys — so
a wrong flow set would have survived this round untouched. The owner reading this is the only
check on that.
