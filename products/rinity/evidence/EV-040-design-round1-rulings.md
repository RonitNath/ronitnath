---
id: EV-040
date: 2026-08-06
provenance: ground-truth
source: owner, design gate ② round-1 review of Penpot "rinity pitch-rinity-1" (F-4 boards)
---
On the queue header's "1 call in progress · Listen" line: "I don't like this here, instead
there should be a separate section at the top listing all active calls with one button to
open it and another to start/stop listening. Also, add an active-call mockup page."

On the call detail's provenance panel (line=overflow, run label=production): "this should
not be here - that's info for the settings page practice-global. Run label = production is
bad. This view you're showing me should be a polished client page. Also, I want a dense
waveform for the audio, and that should be scrubbable - this is a point where part of my
instructions are for updating the mock, and another part are also to update the spec. The
waveform should be split by caller/agent audio, so one is above and the other is below.
ALso, there should be a button which takes you to the person's page in the directory. On
this page, you'd be able to see information about a person longitudinally."

"Lgtm otherwise, build a few more pages. Remember, the layout should be based on what's
appropriate for the page. Don't blindly copy layouts from other pages."

Spec consequences (beyond the mock):
- Call playback is a **dense, scrubbable waveform split by speaker** — agent channel above
  the axis, caller below (→ PKT-05 amendment).
- Client-facing call pages are **polished client surfaces**: no internal ops info. Line/
  answering-model config is practice-global settings (F-7/PKT-03); run labels stay in the
  data model and ISO views, and surface client-side only when a call is not production
  (the queue's TEST chip was accepted).
- New tier-1 surface: **person directory page** — longitudinal view of a person (calls,
  appointments, messages over time), reachable from a call's detail (→ PKT-16).
- Active calls are a first-class queue-page section (open + start/stop listen per call),
  and the active-call view is its own surface (F-11/PKT-10).
