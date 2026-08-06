---
id: EV-041
date: 2026-08-06
provenance: ground-truth
source: owner, design gate ② round-2 review of Penpot "rinity pitch-rinity-1" (queue, detail, active call, person, settings)
---
"The controls are generally too far. A user has to ask, 'where in the page do I need to
search to find this action?' Something like going to the Person's page or start/stop
listening to the call should be a larger button right below the person's name."

"For scrubbing, the words which have not yet been said in the transcript should be a
little more pale. This allows the user to also scan through to notice how far in the
transcript they are. Also, the user should be able to click on the transcript and that
causes them to move position in the playback. This should respect playback state. If
it's playing, then it continues playing from the new position. If paused, it remains
paused but just at the new location."

"'Holds' should instead be 'Booking Holds'."

"Many of these dashboard surfaces should be updated in real time - SSE endpoints.
Anything which could be updated by something else. For instance, the grading is likely
an async post-call process, so may not land immediately. There should be a progress bar
for that so the user knows it's on the way. Same with the main dashboard."

On the queue's active-calls section rows ("New caller · booking a cleaning"): "First
the green dot should be removed. Second, how is the 'new caller booking' derived? Is
there a second LLM? This feature doesn't exist. Instead, it should be being updated in
realtime with the latest sentence, naturally breaking when the text gets too long for
whatever breakpoint it's at."

On the person page's contact panel: "for confirmations here, this should be
'Communication Preference'."

On settings: "part of this requires onboarding. the wait-while-ringing depends on
configuring with the practice's phone provider. Thus, the settings changes visible and
available should be limited to what the practice reasonably should be able to
dynamically change. Further, some of this information is generic knowledge base - it
should be free-form. Yes, we want the practice to put some standard information in as
part of onboarding, but this should all be easily and obviously editable. Also, make a
dedicated page to allow the user to manage which slot availabilities are open. This is
a settings sub-page, and should be visually represented on the sidebar as such."

"Also, for each of these pages, if they have sub-flows or other states, build a mock
for those as well, so I can see the varients (for example, what happens if I click the
edit button on the settings page?) Remove the status dot everywhere btw."

Spec consequences (beyond the mock):
- **Primary actions sit under the identity.** Whatever the page is about (a person, a
  call), its main actions are large buttons directly below the name — never scattered
  into headers or rails where the user must search (→ PKT-01 design language, PKT-05,
  PKT-10, PKT-16).
- **Transcript is a scrub surface** coupled to the waveform: not-yet-played words render
  paler; clicking any word seeks playback to it; seeking preserves play/pause state
  (→ PKT-05).
- **"Booking Holds"** is the client-facing name for DEC-002 holds (→ PKT-05/PKT-10 copy).
- **SSE is the default update transport for console surfaces** — anything another
  process can change updates live. Async post-call grading shows an explicit
  in-progress affordance (progress bar) until the grade lands (→ PKT-01, PKT-05).
  Mirrors the same ruling in ronitnath-universe delta-4.
- **No fabricated derivations.** The active-call summary line implied a second
  intent-summarizing LLM that is not in the spec; a surface may only show what the
  system actually has — here, the live transcript's latest sentence, wrapping naturally
  at the breakpoint (→ PKT-05, PKT-10). Fourth instance of the counters-must-name-
  what-they-witnessed rule.
- **Contact copy**: "Communication Preference", not "Confirmations" (→ PKT-16).
- **Console settings are runtime-changeable only.** Anything requiring the phone
  provider or onboarding (wait-while-ringing seconds, line wiring) is configured during
  onboarding (PKT-07/PKT-09) and is not an editable console setting. Practice knowledge
  is a **free-form knowledge base** — seeded at onboarding, obviously and easily
  editable in the console (→ PKT-03).
- **New settings sub-page: availability management** — the practice manages which slot
  availabilities are open to the agent; sidebar shows settings sub-pages hierarchically
  (→ PKT-03, feeds the PKT-02 bookable pool).
- **Design language: no status dots**, anywhere — states are words in their own color
  (extends the no-status-pills rule; → PKT-01).
- **Design-gate practice: every surface ships its states** — sub-flows and variants
  (edit modes, pending states) get their own mocks alongside the base board.
