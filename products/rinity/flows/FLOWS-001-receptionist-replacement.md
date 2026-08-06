---
id: FLOWS-001
product: rinity
pitch: PITCH-001 (freezes with it)
date: 2026-08-06
status: FROZEN 2026-08-06 with PITCH-001 (tag pitch/rinity-1) — debated (DEBATE-001), extended by owner requirements batch (EV-027..EV-038)
sources: EV-012, EV-013, EV-015, EV-016, EV-019, EV-020, EV-021..EV-026, EV-027..EV-038, DEC-001, DEC-002, EV-011 (engine capabilities), EV-010 (as-built)
---

# FLOWS-001 — receptionist replacement, tier 1 (inbound)

Actors: **CALLER** (patient or prospective patient on the phone), **DESK** (front-desk staff /
practice operator), **OWNER** (practice owner/decision-maker), **ISO** (Isoastra operator).
Tier 1 is inbound-only (EV-023). Flows marked ⟨tier 2+⟩ are named non-goals for tier 1,
listed so the data model leaves room (EV-023).

Audgent is invisible in every flow (EV-015): callers hear "the practice"; DESK/OWNER see only
rinity surfaces.

---

**F-1 The agent answers a call and books a visit** — the flagship flow
- actor: CALLER
- trigger: patient dials the practice's number (new or existing patient)
- outcome: a booked appointment exists in the office's system of record; the call and its
  outcome are visible in the console; the caller was never dead-ended
- surface: outside-web (phone) + machine-only (booking lands via the office's connected path)
- steps:
  1. (phone) call is answered inside the practice's declared hours behavior — no IVR maze,
     the agent greets as the practice
  2. (phone) agent identifies the caller: existing-patient match (name/DOB) or new-patient
     capture (name, callback number, reason)
  3. (phone) agent negotiates a slot **per the office's offer policy** (EV-033: ~2 offers
     per week then advance — anti-enumeration, customizable) from the office's bookable pool
     (in dedicated-slots mode that pool is the agent's reserved slots, not the open
     calendar — EV-032)
  4. (machine) offered slots are **held** the moment they're offered (EV-027, DEC-002); the
     hold resolves to a booking once the caller has supplied enough, or expires; the write
     reconciles through the adapter path the console uses
  5. (phone) agent confirms details back and closes; caller hangs up with a time
  6. (machine) call record (recording, transcript, outcome, booked-appointment link) lands in
     the console feed (F-4)
- branches: caller wants to reschedule/cancel instead (→F-2); question only, no booking
  (→F-3); caller demands a human or the agent judges it should hand off (→F-5); caller
  hangs up mid-flow
- states: no matching patient and caller declines intake → message for DESK (F-5 outcome);
  no acceptable slot → offer waitlist/message, never silent failure; hold beaten by an
  out-of-band PMS write (rare by construction, DEC-002) → re-offer, apologize once, F-4
  conflict item; engine outage → practice-configured fallback (voicemail/forward, a DEC for
  the data gate)
- packets: derived at packet stage
- open: visit-type taxonomy per office (maps to adapter's appointment types); how "the right
  provider" is chosen (continuity vs first-available) — per-office policy config

**F-2 The agent reschedules or cancels an existing visit**
- actor: CALLER
- trigger: patient calls to move or cancel
- outcome: system of record updated; console feed shows the change and why
- surface: outside-web (phone) + machine-only
- steps: identify caller → locate upcoming visit → offer alternatives or confirm
  cancellation → write through adapter → confirm back → land in console feed
- branches: cancellation with no rebooking → flagged for recall ⟨tier 2 outbound⟩; caller
  disputes a visit the system doesn't show
- states: visit not found → hand to DESK with context (F-5); double-booking conflict at
  write → re-offer
- open: cancellation policy enforcement (fees, notice windows) — per-office config or out?

**F-3 The agent answers practice questions**
- actor: CALLER
- trigger: caller asks hours, location/directions, parking, insurance acceptance, service
  questions ("do you do implants?"), pricing ranges
- outcome: correct answer given in the practice's voice; unanswerable questions become
  messages, never improvisation
- surface: outside-web (phone)
- steps: question → answer from the practice's configured knowledge (F-7) → offer to book
  if relevant (→F-1)
- branches: question outside configured knowledge → explicit "I'll have the office get back
  to you" + message (F-5 outcome); clinical advice sought → decline + offer booking/urgent
  guidance per practice policy
- states: knowledge stale (hours changed, doctor left) — staleness is OWNER's to fix via F-7;
  the agent must never contradict the system of record on schedule facts
- resolved (DEBATE-001 COL-3): tier 1 answers from configured practice facts (accepted
  plans included); anything deeper is captured verbatim with a promised follow-up — never a
  stonewall; live eligibility stays a gated integration (EV-022)

**F-4 DESK reviews what the agent did** — the console's new center of gravity
- actor: DESK
- trigger: start of day / between patients / a patient walks in saying "I called earlier"
- outcome: DESK knows every call, its outcome, and what needs human follow-up; nothing the
  agent did is a surprise
- surface: web
- steps:
  1. (web) console shows the call feed: time, caller, intent, outcome (booked/changed/
     message/handed-off), links to the affected appointment
  2. (web) DESK opens a call: transcript + recording + what the agent committed to + the
     call's LLM grade (per-aspect agent-behavior ratings, caller-emotion read, resolution
     efficacy/speed — EV-034)
  3. (web) DESK works the needs-attention queue (messages, failed intents, callbacks)
  4. (web) resolved items are marked done; the queue reaches zero
- branches: DESK disputes an agent action → corrects the appointment on the calendar (F-6)
  and flags the call (feeds agent improvement, ISO-visible)
- states: empty (no calls yet — must not look broken); high-volume day (triage ordering);
  call with no outcome (hang-up) shown honestly
- open: is the needs-attention queue part of the call feed or a separate surface? (design
  gate); retention/PHI posture of transcripts+recordings (data gate, EV-018)

**F-5 The agent hands a call to a human**
- actor: CALLER + DESK
- trigger: caller asks for a human; agent hits its competence boundary; practice-configured
  always-handoff cases (emergencies, billing disputes)
- outcome: warm transfer when the desk is available; otherwise a promise the practice can
  keep — a message with full context, visible in the needs-attention queue
- surface: outside-web (phone) + web (the queue)
- steps: detect handoff condition → attempt transfer per office config → on no-answer,
  capture message + context → land in F-4's queue with priority
- branches: emergency indicators → practice's urgent-case script (config, F-7), never a
  generic brush-off
- states: after-hours (no human exists) → message path with explicit expectation-setting;
  transfer target busy → message path
- open: what transfer mechanics does tier 1 promise (blind vs warm vs message-only)? engine
  supports transfer (EV-011/AC-01 transfer callbacks) — policy is the question

**F-6 DESK/OWNER runs the schedule desk** (exists today; quality bar not met — EV-026)
- actor: DESK (daily), OWNER (occasionally)
- trigger: front-desk work: walk-ins, phone-free bookings, corrections to agent actions
- outcome: the calendar is correct; changes propagate to the system of record
- surface: web
- steps: today's calendar (week/day/providers) → create/edit/cancel visits → changes land
  through the adapter; agent-created visits are visually attributable (who booked this)
- branches: correcting an agent booking (from F-4)
- states: EV-026 defines the bar: it must work the way a human expects under real mouse and
  keyboard use — quick-create must not default to the past, shortcuts must work, drag must
  be trustworthy. Machine walkthroughs don't count as passing (EV-026).
- open: none — this flow's questions are quality questions

**F-7 OWNER configures their practice's agent**
- actor: OWNER (with DESK maintaining day-to-day facts)
- trigger: onboarding (first setup) and whenever reality changes (hours, providers, policies)
- outcome: the agent behaves as *this* practice — its philosophy, not a generic bot (EV-002)
- surface: web
- steps:
  1. (web) practice profile: hours, holidays, location, parking, phone etiquette/voice
  2. (web) knowledge: services, insurance accepted, FAQs in the practice's wording
  3. (web) policies: **answering model** (overflow — agent answers only after the office
     doesn't pick up — or dedicated agent-bookable slots, EV-032), **slot-offer policy**
     (offers per week / advance behavior, EV-033), booking rules (who books with whom,
     buffer rules), handoff rules (F-5), urgent-case script, fallback behavior on outage
  4. (web) preview/test: OWNER hears the agent handle a test call before going live
- branches: config error discovered via a bad call (F-4 dispute) → edit → verify
- states: unconfigured office (defaults must be safe and honest); conflicting rules
- open: config data model is a compatibility surface (BRIEF-001 risk) — versioning?; how
  does "test call" work pre-line-connection (browser test call — engine supports PCM
  browser calls, EV-011/AC-02)

**F-8 OWNER signs up and reaches their own console** — self-serve door (EV-016, EV-022)
- actor: OWNER
- trigger: heard of rinity (marketing, referral); wants it for their practice
- outcome: their own org + office exist; they're in the console configuring (F-7); gated
  integrations visibly locked (EV-022)
- surface: web
- steps:
  1. (web) front door says what rinity is for a high-end practice (EV-024) and what it costs
     (per office, EV-024)
  2. (web) sign up → org + first office created; no Isoastra human required (EV-016)
  3. (web) **onboarding takes the practice's website URL and scrapes everything it can**
     (hours, services, providers, insurance, location, tone) — the owner is asked **only
     for what scraping couldn't determine** (EV-031; spec'd beforehand, a design-gate
     deliverable — the bar is "RCDA has seen good AI product before")
  4. (web) guided into F-7 to review scraped config and fill gaps, with rinity-native
     scheduling available immediately; PMS integration shown as gated (sales call unlock,
     EV-022)
  5. (web) billing established per office before/at go-live (EV-019 — cash flow needs
     this), and only after the browser test call has proven value (C2-4)
- branches: sales-call onboarding path lands in the identical console state (EV-022 — both
  paths first-class); multi-office group adds offices
- states: abandoned mid-signup; payment failure; org exists but no line connected (product
  must still be useful/configurable)
- open: identity door for external customers — current Kanidm member-allowlist is
  Isoastra-internal (EV-010/RF-02); customer identity realm is a data-gate/debate question

**F-9 OWNER connects the practice phone line**
- actor: OWNER (RCDA first, EV-025)
- trigger: configuration done, test call approved (F-7); ready for real traffic
- outcome: real calls reach the agent; the practice can roll back to their old answering
  path at will
- surface: web + outside-web (carrier reality)
- steps: choose answering model (EV-032: full answering, or **overflow** — forward on
  no-answer so the agent works only when the office doesn't pick up) → choose path (new
  number forwarding / port / carrier config per engine's telephony providers, EV-011/AC-01)
  → guided setup with verification call → go-live switch with explicit rollback control
- branches: gated-integration variant: assisted cutover via sales call (EV-022) — likely
  the RCDA path
- states: misconfigured forwarding (detection, not silent dead air); after-go-live cold feet
  (one-click revert)
- open: which telephony paths does tier 1 *productize* (vs engine-supported but not offered)?

**F-10 ISO operates the fleet**
- actor: ISO
- trigger: daily operation; an office's calls degrade; a new office goes live
- outcome: Isoastra sees health, cost, and quality across offices without touching audgent's
  raw console for routine work (EV-015 — audgent stays internal; rinity is also where ISO
  routine ops should live so customer-visible state has one source of truth)
- surface: web (rinity operator surface) + outside-web (audgent/observability for deep work)
- steps: fleet view (calls, error rates, capacity vs the alien numbers in EV-011 §4) →
  **cost analytics: cost per call, component-level cost drivers, per-office cost
  projections** (EV-035) → drill into an office's failed or low-graded calls (grades,
  EV-034) → act (config fix, engine escalation)
- branches: capacity limit approached (EV-011: warn at 6 concurrent) → scaling decision;
  cost anomaly → drill down to the calls (including **test traffic** — every test run is a
  saved, labeled, reviewable record inside the same cost ledger, EV-038; run labeling may
  require an audgent change)
- states: an office with zero calls (line misconfig? seasonal?) — surfaced, not silent
- open: how much of this is rinity UI vs existing Grafana/observability in tier 1?

**F-11 Listening in live on a call in progress** (EV-037)
- actor: DESK/OWNER (own office), ISO (any office)
- trigger: a call is in progress and someone needs ears on it — training, spot-checking a
  new config, an escalation forming
- outcome: live one-way audio of the call in the browser; the caller experience is
  unaffected
- surface: web (+ engine tap)
- steps: in-progress calls visible in the console (live status) → open one → hear it live
  → optionally trigger handoff (F-5) if it's going wrong
- branches: call ends while listening → lands as the normal F-4 record
- states: no engine tap available (whether audgent exposes live audio out of the call path
  is an open engine question — data gate); concurrent listeners
- open: access model (practice on own calls vs ISO on any) and whether listen-in events are
  themselves audited — data gate

**F-12 OWNER reads the practice's analytics** (EV-036)
- actor: OWNER (DESK occasionally)
- trigger: monthly "what am I paying for" moment; a quality doubt; a staffing decision
- outcome: OWNER sees call volume (answered, booked, handed off, missed) and quality (grade
  trends by aspect, EV-034) for their office
- surface: web
- steps: console analytics view → volume over time, outcome mix, grade trends → drill into
  a specific low-graded call (→F-4 record)
- branches: quality dips after a config change → back to F-7
- states: sparse data (first weeks — must not look broken); overflow mode (EV-032) framing:
  volume shown is *calls the office would otherwise have missed*
- open: which aggregates are tier-1 vs later polish — design gate

---

## Named non-goals (tier 1)

- **Outbound flows** — confirmations, recall, reminder campaigns, callback-return (⟨tier 2+⟩,
  EV-023). Schema leaves room: call direction, consent/quiet-hours.
- **Multi-vertical variants** (EV-017) — dental-specific knowledge lives in config, not code.
- **Patient-facing web surface** — CALLER's surface is the phone only in tier 1; no patient
  portal/web-booking. (Named so the actor isn't silently dropped: CALLER has F-1/F-2/F-3/F-5.)
- **Per-call/usage billing surface** (EV-024 — per-office flat).

## For the data gate

Call/transcript/recording retention + PHI posture (EV-018); customer identity realm vs
internal Kanidm door (interim-door exception scoped by DEC-001); office config schema +
versioning; attribution model (who booked: agent/DESK/ISO); the rinity↔audgent seam
contract (EV-011 §2 vs EV-010 §3 — the missing wiring is the load-bearing seam of the
whole bet). From the 2026-08-06 owner batch: **hold semantics** (TTL, scope, what beats a
hold — DEC-002); **PMS sync freshness contract** (which adapters push vs poll, staleness
bound the agent may act on — EV-028); **dedicated-slots modeling** (office-open ≠
agent-bookable, EV-032); **grading schema** (aspects, scales, grader cost attribution —
EV-034); **cost attribution + projection model** (engine usage/cost → rinity per-call/
per-office, EV-035); **test-run labeling** (test vs real traffic separable in the cost
ledger — likely an audgent contract change, EV-038); **live listen-in transport + access
model + audit** (EV-037).
