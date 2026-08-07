---
id: FLOWS-001
product: audgent
bet: 1 — internal reshaping
pitch: PITCH-001 (not yet written; freezes with it)
date: 2026-08-06
status: draft — for owner strike/add, then debate, then freeze with PITCH-001
sources: BRIEF-001; EV-009..EV-014, EV-017, EV-019, EV-021, EV-022
---

# FLOWS-001 — audgent internal reshaping

Actors: **OWNER** (the only human; inspects, does not configure — EV-012, EV-014),
**AGENT** (worker agents; the primary interfacing surface — EV-006, EV-011),
**RINITY** (the front desk service; the only consuming product — EV-009, EV-017).

The inversion this bet encodes: in the inherited product a human configures and machines
execute. Here machines configure and a human watches. Every OWNER flow below is a read; every
authoring flow belongs to AGENT.

Two structural notes:

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
- branches: the call reveals a configuration problem → F-6 (direct an agent to fix it)
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
- outcome: OWNER can read the live configuration of providers, models, workflows and numbers
  without editing anything
- surface: web
- steps:
  1. (web) configuration view: what providers exist, which are in use, what each workflow
     is pinned to
  2. (web) open one and read it, including which credential it uses (not the secret)
  3. (web) from any configuration item, reach the history of who changed it (→F-5)
- branches: the configuration is wrong → F-6
- states: a provider configured but never exercised; a credential present but invalid — the
  view must distinguish "set up" from "known to work" (this is what F-8's test verb feeds)
- packets: derived at packet stage
- open: does the reshaped surface keep the existing console's configuration screens as
  read-only, or is this a new surface (BRIEF-001/OQ-5)? The screens' current organization
  assumes data entry

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

**F-6 OWNER corrects the system by directing an agent** ⟨derived — confirm or strike⟩
- actor: OWNER (+ AGENT)
- trigger: any of F-1..F-5 surfaced something wrong
- outcome: the thing is fixed, by an agent, and the fix appears in F-5
- surface: outside-web (a conversation with an agent) + machine-only (the change itself)
- steps:
  1. (web) OWNER identifies the specific thing that is wrong and can name it precisely enough
     to hand off — an identifier, not a description
  2. (outside-web) OWNER tells an agent
  3. (machine) agent makes the change (→F-8 or F-7)
  4. (web) the change appears in F-5, and OWNER can confirm it landed
- branches: OWNER decides to change it himself → **there is no such path by design** (EV-012);
  if that turns out to be intolerable, EV-012 is what needs revisiting, not this flow
- states: OWNER can see a problem but cannot name the object precisely enough to hand it off —
  this is the failure mode that makes an inspection-only surface unusable, and it is what the
  identifiers in F-1..F-5 exist to prevent
- packets: derived at packet stage
- open: **this flow was derived, not stated.** It follows from EV-012 (config is agent work)
  plus EV-014 (the human surface is five reads) — if the owner sees something wrong, the loop
  has to close somewhere. Written per `flow-addendum.md` rule 1 (a journey outside the web UI
  is still a flow). Strike it if the intended answer is that OWNER simply edits directly

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

---

## Named non-goals (actors and journeys deliberately absent)

- **A customer or tenant.** No one outside signs in to audgent. Products plug in with keys;
  customers are rinity's, above this line (EV-017, rinity/EV-015).
- **A human configuring anything.** There is no OWNER authoring flow — that is the point of
  EV-012, and F-6 is the escape valve.
- **AGENT diagnosing a failing production call.** A plausible fifth agent verb (read
  transcripts and costs programmatically to investigate), but it was not discussed, so it is
  out by default under EV-021. Flagged here rather than silently added — worth an explicit
  yes or no, since it is the one omission likely to be missed once agents are working in the
  system.
- **Phonetic misrecognition repair.** Bet 2 (EV-022). No flow here, and it must not leak into
  this pitch, its packets, or its gates.
- **The one-org migration.** A work packet, not a journey (rule 2).
