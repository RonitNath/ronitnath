---
id: FLOWS-001
product: audgent
bet: 1 — internal reshaping
pitch: PITCH-001 (not yet written; freezes with it)
date: 2026-08-06
status: draft rev 2 — owner review round 1 folded in (EV-024..EV-028); then debate, then freeze with PITCH-001
sources: BRIEF-001; EV-009..EV-014, EV-017, EV-019, EV-021, EV-022, EV-024..EV-028
---

# FLOWS-001 — audgent internal reshaping

Actors: **OWNER** (the only human; predominantly inspects, edits rarely but really —
EV-012, EV-014, EV-024), **AGENT** (worker agents; the primary interfacing surface — EV-006,
EV-011, EV-028), **RINITY** (the front desk service; the only consuming product — EV-009,
EV-017).

The inversion this bet encodes: in the inherited product a human configures and machines
execute. Here machines configure and a human watches — but the human keeps the keys. Agents
are the *normal* path, not the *only* path (EV-024); the design problem is keeping an
infrequent edit path available without letting it shape the layout, which is what the
inherited console gets backwards.

Three structural notes:

- **The surfaces stay.** The no-code flow builder, model configuration, and the rest are
  re-cut for one org, not replaced (EV-027, EV-018). Only three views are genuinely new
  construction: agent activity (F-5), the production/agent-test cost split (F-3), and
  wire-level drill-down (F-10).

- **Scoping is gone from every flow.** Each OWNER read spans the whole system, not a
  workspace (EV-010, EV-017). Where the current console would ask "which org", these flows
  ask nothing.
- **The tenancy teardown itself is not a flow.** It is one-time machine-mediated work — a
  work packet, per `flow-addendum.md` rule 2. Drawing it as a journey would invent a
  migration UI nobody will use.

---

**F-1 OWNER sees what is happening right now**
- actor: OWNER
- trigger: wondering whether the system is doing anything, or whether something just broke
- outcome: OWNER knows every call currently in flight across the system, and that the answer
  is complete rather than filtered
- surface: web
- steps:
  1. (web) open the system view; calls in flight are listed — who, which workflow, how long,
     which product originated it, production or agent-test
  2. (web) open one in-flight call to watch it progress
- branches: nothing in flight → F-2 (what happened recently); something looks wrong → F-2 on
  that call once it ends
- states: **empty is the common case and must not read as broken** (EV-019 — nothing is
  carrying calls today); many concurrent calls, mostly agent-test, must not bury the one
  production call; a call whose engine leg died mid-flight shown honestly, not silently dropped
- packets: derived at packet stage
- open: does "live" mean a listing that refreshes, or genuine per-turn visibility into an
  active call? The second is a much larger commitment and was not discussed (EV-021)

**F-2 OWNER reviews a call that already happened**
- actor: OWNER
- trigger: a call is reported wrong, or OWNER is spot-checking agent behavior
- outcome: OWNER can reconstruct what the agent said and did, and why
- surface: web
- steps:
  1. (web) call log across the whole system, newest first, labelled production vs agent-test
  2. (web) open a call: transcript, recording, workflow and version it ran, outcome, cost
  3. (web) from the call, reach the configuration it ran under (→F-4)
  4. (web) when the transcript doesn't explain it, descend to the wire (→F-10)
- branches: the call reveals a configuration problem → F-6 (fix it, or have an agent fix it);
  the call looks fine but the work didn't happen → F-10, which is the case the transcript
  cannot show
- states: transcript present but recording missing; a run that never completed; a run whose
  workflow definition has since changed — the log must show the version that actually ran,
  not the current one
- packets: derived at packet stage
- open: retention — how long calls, transcripts and recordings are kept, and whether
  agent-test runs are kept as long as production ones (data gate)

**F-3 OWNER checks what it is costing**
- actor: OWNER
- trigger: spend looks high, or a periodic look
- outcome: OWNER can say how much was production and how much was agents testing on his
  behalf, without reconstructing it by hand (EV-013)
- surface: web
- steps:
  1. (web) cost view, split production vs agent-test at the top level
  2. (web) drill into the agent-test side: which agent, which activity, which runs
  3. (web) drill into a cost line down to the individual call (→F-2)
- branches: agent-test spend is disproportionate → F-6 (direct an agent to change how it
  tests, or change what the cheap path is)
- states: unattributed runs — a run that predates labelling, or one whose origin can't be
  determined, must appear in a visible "unattributed" bucket rather than being silently
  assigned to either side
- packets: derived at packet stage
- open: is the production/agent-test split a property of the credential, the run, or declared
  per call (BRIEF-001/OQ-4)? Credential-derived is self-enforcing; declared is wrong the first
  time an agent forgets

**F-4 OWNER inspects the current configuration**
- actor: OWNER
- trigger: needing to know what the system is actually set to — before judging a call, or
  after an agent reports a change
- outcome: OWNER can read the live configuration — above all, **which providers are in use**
  (EV-025) — and can change it directly if he chooses to
- surface: web
- steps:
  1. (web) configuration view: what providers exist, which are in use, what each workflow
     is pinned to
  2. (web) open one and read it, including which credential it uses (not the secret)
  3. (web) from any configuration item, reach the history of who changed it (→F-5)
- branches: the configuration is wrong and OWNER edits it himself — **rare but real, and not
  a degraded path** (EV-024); or he hands it to an agent (→F-6). An edit OWNER makes appears
  in F-5 alongside the agents' changes, attributed to him
- states: a provider configured but never exercised; a credential present but invalid — the
  view must distinguish "set up" from "known to work" (this is what F-8's test verb feeds);
  an OWNER edit that contradicts what an agent is mid-way through doing
- packets: derived at packet stage
- resolved (EV-027): the existing surfaces stay — the no-code flow builder, model
  configuration and the rest are reconfigured for one org, not replaced. Resolves
  BRIEF-001/OQ-5
- open: with editing retained (EV-024) but rare, how does the surface stay read-shaped
  without hiding the edit path? A design-gate question, and the sharpest one in the bet

**F-5 OWNER reviews what the agents have been up to**
- actor: OWNER
- trigger: something changed and OWNER didn't change it; or a periodic check on his workers
- outcome: OWNER can see the sequence of configuration changes agents made, with enough
  context to judge each one
- surface: web
- steps:
  1. (web) activity view: changes in time order — which agent, what changed, from what to what
  2. (web) open one change: the before/after, and the runs that followed it (→F-2)
- branches: a change looks wrong → F-6
- states: a burst of changes from one agent session shown as a session, not as noise; a change
  whose author can't be attributed
- packets: derived at packet stage
- open: **does a configuration change history exist today at all?** If not this is a new
  data-model commitment and the largest single unknown in the bet (BRIEF-001/OQ-6). It is also
  the read with no equivalent in the inherited console — it exists because the owner stopped
  configuring the system himself

**F-6 OWNER corrects the system by directing an agent**
- actor: OWNER (+ AGENT)
- trigger: any of F-1..F-5 or F-10 surfaced something wrong, and OWNER would rather delegate
  the fix than make it
- outcome: the thing is fixed, by an agent, and the fix appears in F-5
- surface: outside-web (a conversation with an agent) + machine-only (the change itself)
- steps:
  1. (web) OWNER identifies the specific thing that is wrong and can name it precisely enough
     to hand off — an identifier, not a description
  2. (outside-web) OWNER tells an agent
  3. (machine) agent makes the change (→F-8 or F-7)
  4. (web) the change appears in F-5, and OWNER can confirm it landed
- branches: OWNER changes it himself instead (→F-4 branch) — the two paths coexist, and which
  one he takes is a matter of convenience, not capability (EV-024); the agent's fix is wrong →
  back to step 1 with the evidence from F-10
- states: OWNER can see a problem but cannot name the object precisely enough to hand it off —
  the failure mode this flow exists to prevent, and what the identifiers in F-1..F-5 and F-10
  are for; OWNER and an agent editing the same thing at once
- packets: derived at packet stage
- resolved (EV-024): this flow was derived rather than stated, and the owner confirmed it
  while correcting its premise — direct editing survives, so F-6 is the delegation path, not
  the only way to close the loop

**F-7 AGENT builds a workflow and proves it works**
- actor: AGENT
- trigger: a product needs a new voice agent, or an existing one needs changing
- outcome: a workflow exists, has been exercised, and the agent knows from a verdict — not a
  guess — whether it behaves
- surface: machine-only
- steps:
  1. (machine) agent creates or edits the workflow definition
  2. (machine) agent runs it against a test path
  3. (machine) agent reads back a verdict: what happened, what the agent-under-test said, pass
     or fail against what was expected
  4. (machine) agent iterates on the verdict without a human confirming anything
  5. (web) the work is visible to OWNER in F-5, and its cost lands on the agent-test side (F-3)
- branches: verdict is ambiguous → the agent must be able to reach the full transcript, not
  only a summary; the workflow is ready → the product points at it
- states: a test run that never connects; a verdict that says "ran" but not "was correct" —
  the distinction between *executed* and *behaved* is the whole value of step 3
- packets: derived at packet stage
- open: **what is the verdict, concretely?** Is the T9 harness / call-flow evals the substrate,
  or is this a new agent-facing capability (BRIEF-001/OQ-3)? Without a verdict contract,
  "test workflow" is just a trigger, and the two test verbs are the load-bearing half of EV-011

**F-8 AGENT configures a provider and proves it works**
- actor: AGENT
- trigger: a new provider is needed, a key rotates, or a provider starts failing
- outcome: the provider is configured and confirmed working, with no human touching a form
- surface: machine-only
- steps:
  1. (machine) agent sets the provider configuration and its credential
  2. (machine) agent exercises it — the smallest real call that proves the credential and the
     path, not a syntactic check
  3. (machine) agent reads back working / not working, and why if not
  4. (web) the change appears in F-5; the configuration reads as "known to work" in F-4
- branches: not working → agent iterates or escalates to OWNER (which is F-6 in reverse)
- states: credential valid but the provider is down; provider configured but never exercised —
  F-4 must show that as an unproven state rather than as success
- packets: derived at packet stage
- open: what does an agent authenticate as (BRIEF-001/OQ-2)? The current service-principal and
  scope machinery was built for customer backend integrations, not for the owner's workers

**F-9 RINITY runs production traffic**
- actor: RINITY (machine)
- trigger: a real call, inbound or outbound, through the front desk service
- outcome: the call runs; it is attributed to production; it appears in F-1 live and F-2
  afterwards; its cost lands on the production side of F-3
- surface: machine-only
- steps:
  1. (machine) rinity authenticates with its own product-level key (EV-017)
  2. (machine) rinity originates or receives a call against a workflow
  3. (machine) the run is labelled production by virtue of the credential
  4. (web) OWNER sees it in F-1/F-2/F-3 with no filtering
- branches: a second product arrives → it gets its own key, not a tenant (EV-017); that is a
  signal to revisit, not something to design for now (EV-009)
- states: rinity's key revoked or expired; a run that can't be attributed to a product (see
  F-3's unattributed bucket)
- packets: derived at packet stage
- open: does rinity's integration need to change at all for the one-org move, or is the seam
  unchanged from its side? (data gate — it is the only external consumer of the teardown)

**F-10 OWNER drills into the wire to find out why something failed** — the new capability
- actor: OWNER
- trigger: a call failed, or worse, a call *succeeded* and the work didn't happen — the agent
  sounded fine and the tool call never landed (EV-026)
- outcome: OWNER can name the actual cause — this request was sent, this is what came back,
  and that is why — without reasoning from a transcript
- surface: web
- steps:
  1. (web) from a call (F-2), open its boundary record: every outbound request and its
     response, in order, against the provider and against tool endpoints
  2. (web) locate the failure and read the response verbatim — the rejection, the error body,
     the status
  3. (web) read what was sent immediately before it, which is usually where the cause is
  4. (web) from the cause, reach the configuration that produced it (→F-4), and fix it
     directly or hand it off (→F-6)
- branches: the cause is a provider rejecting our request (bad schema, bad credential, bad
  payload) → configuration fix; the cause is the provider itself failing or degrading → a
  provider-choice question, which is the main configuration decision there is (EV-025); no
  request was sent at all → the fault is upstream in the workflow, not on the wire
- states: payloads large enough that the view must be navigable rather than dumped; a boundary
  record truncated or expired by retention; **secrets and PHI inside captured payloads**, which
  is why this cannot simply be raw logging; a run with hundreds of turns
- packets: derived at packet stage
- open: retention and redaction posture for captured request/response bodies — how long, what
  is scrubbed, and whether agent-test runs and production runs are treated alike. This is a
  data-gate commitment and the largest new one in the bet (EV-026)
- open: is capture always-on, or armed per workflow / per run? Always-on is what makes it
  useful for the failure nobody predicted, and is also what makes the volume and retention
  question real

**F-11 AGENT diagnoses a failure**
- actor: AGENT
- trigger: a run failed, a test verdict came back bad (F-7, F-8), or OWNER handed over an
  investigation (F-6)
- outcome: the agent knows the cause and can act on it, without a human reading anything to it
- surface: machine-only
- steps:
  1. (machine) agent reads the run: outcome, transcript, cost, configuration it ran under
  2. (machine) agent reads the same boundary record F-10 renders — requests, responses, order
  3. (machine) agent identifies the cause and either fixes it (→F-8, F-7) or reports it to
     OWNER with the evidence
- branches: cause is outside audgent (the tool endpoint itself, the product) → report, don't
  fix; cause is ambiguous → the agent needs the full payloads, not a summary, which is the
  requirement that decides this flow's API shape
- states: evidence already expired under retention; payloads too large to read whole — the
  agent needs to query the record, not receive it
- packets: derived at packet stage
- resolved (EV-028): admitted as the fifth agent verb, having been listed as a non-goal in
  rev 1
- open: **the same record serves F-10 and F-11.** Building it for the UI first and adding an
  API later is the predictable mistake — the agent path constrains the shape more (it needs
  query, not render), so it should lead

---

## Named non-goals (actors and journeys deliberately absent)

- **A customer or tenant.** No one outside signs in to audgent. Products plug in with keys;
  customers are rinity's, above this line (EV-017, rinity/EV-015).
- **A *self-serve* configuration journey.** OWNER can edit (EV-024) and the existing surfaces
  survive (EV-027), but nothing here is shaped for a stranger arriving with their own vendor
  keys to set themselves up — that is the BYOK model EV-012 rejects. The edit path is an
  exception path for one person who already owns everything.
- ~~AGENT diagnosing a failing production call~~ — **admitted as F-11** by EV-028. Kept
  visible here because it was rev 1's flagged omission and the flag worked.
- **Phonetic misrecognition repair.** Bet 2 (EV-022). No flow here, and it must not leak into
  this pitch, its packets, or its gates.
- **The one-org migration.** A work packet, not a journey (rule 2).
