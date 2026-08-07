---
gate: design (session ② of PITCH-001's bound)
product: rinity
pitch: pitch/rinity-1
date: 2026-08-06
status: SIGNED OFF 2026-08-06 (EV-047 "lgtm … I'll accept capsules, your availability ideation was fine, round 5 was fine" + EV-048 "lgtm" and the direction to the data gate). Session ② closes over nine build→review rounds in one day.
artifact: Penpot "rinity pitch-rinity-1" — file 310e8e79-e53d-81b8-8008-7158170e68e9, team ronitnath.com, agent identity rinity-design
generators: none — boards were accreted through incremental MCP scripts (session scratchpad, mirrored to nexus ~/tmp/rinity-design/), not a regenerable script set. The Penpot file is the artifact of record; this differs from the pilot's design/penpot/ discipline and is accepted for this gate.
---

# Design gate — rinity console

Nine rounds of build → owner corrections → evidence + packet deltas → rebuild, all on
2026-08-06. Rulings are EV-040..EV-048; every one also landed as deltas in rinity
`changes/pitch-rinity-1/` under the two-track discipline. The design language inherits the
pilot's laws (DEC-012 five-size type ladder, no eyebrows, no explanatory quips, states as
words) and extended them with rinity-specific rulings below.

## What the file covers at close

- **Call-review queue (F-4/PKT-05)** — base 1440 dark, light, mobile, tablet, ultrawide.
  Active-calls section at top (open + start/stop listen per call, live latest-sentence rows);
  outcome chips and NEEDS ATTENTION badges as tinted capsules (the accepted exception, EV-047).
- **Call detail** — polished client surface, no internal ops info (EV-040); dense scrubbable
  waveform split by speaker (agent above / caller below); transcript as a scrub surface
  (unplayed words paler, click-a-word seeks, play/pause preserved — EV-041); no outcome chip,
  commitments panel is the evidence (EV-042); grade-pending progress affordance.
- **Active call (F-11/PKT-10)** — liveness by motion, no LIVE badge, ongoing duration top
  right (EV-042); primary actions as large buttons under the identity (EV-041).
- **Person directory page (PKT-16)** — longitudinal view (calls, appointments, messages),
  reachable from call detail (EV-040); "Communication Preference" copy (EV-041).
- **Schedule desk family (PKT-06)** — Day · 3 days · Week · Month · Agenda; weekend columns
  don't exist unless availability opens them (the calendar renders the configured shape,
  EV-044); provider legend as tint-selected buttons with Select/Deselect All (EV-045);
  color-coded events, side-by-side same-time providers, drag-create, popover edit
  (owner-tuned spacing: panel ends where content ends), drag-reschedule, ‹ › period cycling;
  office picker under the practice name (EV-047 §6).
- **Settings** — runtime-changeable only (EV-041); General incl. the PRACTICE WEBSITE
  re-scrape section (EV-047); knowledge base as a structured per-topic surface with
  provenance and the captured-questions queue (EV-042); availability v2: per-provider weekly
  template whose union of open days *causes* the calendar's columns, plus a dated "Time away"
  list with booked-visit collision handling (EV-047 §5). Inline editing everywhere — click
  text, type, auto-saved, no modals, no Save buttons (EV-043).
- **Onboarding (F-8/PKT-07)** — front door (promise + per-office price before identity
  capture), streaming scrape, applied-vs-approval review with inline queue editing,
  ask-only-gaps console; wired as a clickable flow.
- **Call-tree page** — all regular agent pathways as one symbolic-logic flow chart with
  shared guard nodes (EV-045), rebuilt to carry the full EV-046 call protocol: phone-index
  greeting with "is this {person}?", follow-up ask, disclosure line (default off), mid-call
  hold revocation ("my manager just notified me this slot is closed"), sudden-hangup resume,
  multi-matter handling, tool-latency acknowledgment.

## Rulings that reached past the drawing (the gate doing its job)

1. **SSE is the default update transport** — anything another process can change updates
   live; async work (grading, summaries) shows explicit in-progress affordances (EV-041).
2. **No fabricated derivations** — the active-call "booking a cleaning" line implied an LLM
   that didn't exist; surfaces show only what the system has (EV-041; fourth instance of
   counters-must-name-what-they-witnessed).
3. **Component gallery struck from the design gate** for all products; layers renumbered in
   `system/design-gate.md` (EV-043).
4. **The call protocol batch (EV-046)** — eleven product rulings (phone index, summaries not
   transcripts, holds expire on drop, sudden-hangup 1 h resume, booking variants,
   multilingual open item, everything-is-a-default, disclosure, tool latency, multi-matter,
   follow-up ask) — arrived as design-review reactions and became packet deltas; they are
   the data gate's densest input.
5. **Scrape is not onboarding-only** — settings re-scrape for established clients, merging
   against history via the approval queue (EV-047).
6. **Capsule exception scoped** — the cross-product no-status-pills law stays a strong
   default; rinity's outcome chips/needs-attention badges are the owner-ruled exception.
7. **Build execution model** — the rinity build runs on the workorder system
   (`context/procedures/coordination.md`), superseding the pi-orchestration default; packets
   remain the spec units, each workorder cites its PKT(s) + pitch tag (EV-048).

## Deliberately not built (recorded so absence isn't mistaken for a gap)

- F-8 signup/onboarding **breakpoint boards** and the **abandoned/resume state** boards
  (named in PKT-07's ux line) — coverage was accepted at round 9 without them; they are
  build-time states the packets fully specify.
- **Magnification states** — excluded from the base app by ruling (EV-044).
- **Multilingual surfaces** — open item awaiting the capability check + focused design pass
  (EV-046 §7); the data model reserves `language` on call records.
- Notification-proposal system for schedule changes — later work, named so the schedule
  surface doesn't design it out (EV-043).

## Constraint carried to the data gate

The accepted surfaces promise data the control plane does not yet persist: call records,
grades, summaries, persons with a phone index, booking holds with mid-call revocation,
provenance-carrying config/KB, scrape tasks. EV-010 records none of it exists. The data-model
brief (rinity `changes/pitch-rinity-1/data-model.md`) is the answer; its sign-off record will
live beside this one in `data/`.
