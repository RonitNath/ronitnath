---
id: FLOWS-001
product: voice
bet: bet 1 — the ladder
date: 2026-08-06
status: draft (freezes with PITCH-001)
---

# FLOWS-001 — the ladder

Addendum to PITCH-001. In scope for debate (SYS-DEC-004): the flow set itself may be struck,
added to or reordered before freeze.

## Actors

| Actor | Standing |
| --- | --- |
| **Owner** | The only human principal. Authenticates through Kanidm (EV-039). Every flow below is his. |
| Consuming systems (rinity et al.) | **Named non-goal for this bet.** There is no consumer; the plug-in seam is designed at graduation against a real one (EV-012, EV-037). Zero flows, deliberately. |
| End callers | **Named non-goal.** They exist inside recordings and conversations as *subjects*, never as users of this product (EV-012). Zero flows. |
| Agent workers | Build the rungs. Their work is packets, not journeys (rule 2) — except F-10, where the owner's greenlight is the journey and the agent is the counterparty. |

## Rung column

Each flow names the rung at which it first exists. Flows above rung 2 are written now because
the pitch freezes the whole ladder, not because they are built now — no rung N+1 work begins
before rung N is greenlit (EV-038).

---

**F-01  Sign in**
- actor: Owner
- trigger: The owner opens voice.ronitnath.com to look at something.
- outcome: An authenticated session; the service knows who is acting.
- surface: web
- rung: 1
- steps:
  1. Hits any URL on voice.ronitnath.com — unauthenticated, is redirected (web)
  2. Authenticates against Kanidm (Kanidm's surface, not ours)
  3. Returns to the URL originally requested (web)
- branches: Already-valid session — step 1 proceeds directly, no interruption.
- states: *expired* — session lapses mid-scrub or mid-conversation, and the return must land back where he was, not at a root page. *error* — Kanidm unreachable; the service says so plainly rather than failing open.
- packets: derived at the packet station
- open: Does a live conversation survive session expiry, or is it dropped? (data gate)

**F-02  Put audio into the system**
- actor: Owner
- trigger: He has a recording he wants the system to hold and measure — an existing file from anywhere.
- outcome: The clip is stored on the filesystem, permanent, with a row on the timeline and its basic statistics computed (EV-014, EV-033).
- surface: web
- rung: 1
- steps:
  1. Selects one or more audio files (web)
  2. The service stores the bytes, records the clip, computes duration/sample rate/channels/size and a waveform (web, background)
  3. The clip appears in the library with its statistics (web)
- branches: Several files at once — each becomes its own clip, none blocks another. A format the system cannot decode.
- states: *empty* — the library before the first clip; it must read as "nothing here yet", not as a broken page. *error* — undecodable or truncated file, named specifically. *conflict* — the same file uploaded twice; is it one clip or two? (see open)
- packets: derived at the packet station
- open: Is duplicate detection by content hash wanted, or is a second upload simply a second clip? (data gate) — bears on EV-033's append-only corpus.

**F-03  Record from the browser**
- actor: Owner
- trigger: He wants to say something into the system directly rather than find a file.
- outcome: A clip indistinguishable from an uploaded one, stored permanently.
- surface: web
- rung: 1 (capture) / 2 (as the streaming path's source)
- steps:
  1. Grants microphone access (browser)
  2. Records, sees level and elapsed time while recording (web)
  3. Stops; the clip is stored and enters the library exactly as F-02's do (web)
- branches: Cancels mid-recording — nothing is stored. Later, at rung 2, the same capture feeds the streaming path live instead of only landing as a file.
- states: *error* — microphone permission denied, or no input device. *empty* — a recording with no audible content still becomes a clip; silence is data.
- packets: derived at the packet station
- open: Does browser capture belong to rung 1 at all, or does rung 1 accept only files as EV-014 literally states? Cheapest reading: capture lands at rung 1 because the streaming path at rung 2 needs it and it is the only way to generate fresh corpus without leaving the app.

**F-04  Look at what was actually said**
- actor: Owner
- trigger: A clip has been transcribed and he wants to see whether the machine heard it correctly.
- outcome: He has formed a judgment about transcript quality against the audio itself (EV-016).
- surface: web
- rung: 2
- steps:
  1. Opens a clip (web)
  2. Sees the waveform, the transcript, and speaker attribution together on one timeline (web)
  3. Scrubs — position, waveform, transcript and speaker stay locked together in both directions (web)
  4. Reads the timing layer against the audio: per-word arrival, end-of-sentence, end-of-turn, each shown where it was detected (web) (EV-017)
- branches: Clicks a word to jump the playhead. Plays a single segment in isolation. Compares the bulk transcript against the streaming transcript of the same clip (EV-015) — the divergence is the point.
- states: *empty* — clip stored but not yet transcribed; the surface must say "not transcribed", never render a blank transcript that reads as silence (EV-009). *error* — the STT provider failed; the failure is shown on the timeline where it happened.
- packets: derived at the packet station
- open: How are bulk and streaming transcripts of one clip held — two transcripts on one timeline, or one with revision history? (data gate; bears directly on EV-041)

**F-05  Run a configuration over a clip**
- actor: Owner
- trigger: He wants to know how a particular provider/model/threshold combination handles some audio (EV-019, EV-034, EV-035).
- outcome: A run exists, attributed to its exact configuration and code version, with its latencies and costs recorded (EV-027, EV-041).
- surface: web
- rung: 2
- steps:
  1. Picks a clip, or a set of clips (web)
  2. Picks a configuration — provider, model, thresholds — or a matrix of them (web)
  3. Starts the run; watches progress, since hosted providers make this take real time and cost real money (web)
  4. The run lands on the timeline beside the clip, never replacing an earlier run (web)
- branches: A matrix — one clip across many configurations, or many clips through one. Re-running an identical configuration to see variance.
- states: *error* — provider rejects, rate-limits, or times out; recorded as a failed run with its cost, not discarded. *conflict* — a configuration that names a model the provider no longer serves.
- packets: derived at the packet station
- open: Does starting a run require a spend confirmation, or is a budget ceiling set once? (data gate)

**F-06  Compare runs**
- actor: Owner
- trigger: He has several runs and wants to know which configuration is better, on latency, cost and correctness (EV-019, EV-027, EV-041).
- outcome: A defensible judgment about a configuration — the thresholds worth keeping.
- surface: web
- rung: 2
- steps:
  1. Selects runs — same clip across configurations, or one configuration across the corpus (web)
  2. Sees them side by side: latency distributions, per-word arrival, cost, and where transcripts disagree (web)
  3. Drills from any comparison back into the clip timeline at the moment in question (F-04) (web)
- branches: Comparing runs produced under different code versions — the surface must say so rather than compare silently (EV-041).
- states: *conflict* — runs are not comparable because the measurement schema changed between them; the system must declare this, not average across it. *empty* — only one run exists; comparison is unavailable, said plainly.
- packets: derived at the packet station
- open: What exactly makes two runs comparable, and where is that predicate written? This is the single question EV-041 raises and the reason the debate is scoped to the timeline model.

**F-07  Find where it went wrong**
- actor: Owner
- trigger: Something in a run is wrong — a bad transcript, a late first token, a cost spike, a wrong speaker.
- outcome: The failure is localized to one component, which is the whole remedy for console's death (EV-029).
- surface: web
- rung: 2 (and deepens at every rung above)
- steps:
  1. Opens the run (web)
  2. Sees the per-component trace along the same timeline as the audio — what each component received, emitted, and when (web)
  3. Identifies the component that produced the wrong output or the delay (web)
  4. Re-runs that clip against a changed configuration to test the hypothesis (F-05) (web)
- branches: The failure is in a hosted provider, not in our code — the trace must distinguish these, since EV-035 puts model compute outside our process.
- states: *empty* — a component produced nothing; that is a finding, and must render as absence, not as a gap. *error* — trace data itself is missing for a span; say so rather than draw a continuous line across it (EV-009).
- packets: derived at the packet station
- open: Retention of traces is settled by EV-033 (forever) for audio — does it extend to traces and runs? Presumed yes; confirm at the data gate.

**F-08  Talk to it in text**
- actor: Owner
- trigger: He wants to hold a conversation where he speaks and it replies in writing (EV-018).
- outcome: A conversation exists as a first-class recorded artifact, with the two latency baselines measured: speech-stop → first token, and detected end-of-turn → first token.
- surface: web
- rung: 3
- steps:
  1. Starts a session; audio streams from the browser (web)
  2. Speaks; sees the streaming transcript arrive and the turn boundary get detected, live (web)
  3. Sees the model's reply stream in as text (web)
  4. Ends the session; the whole thing is stored as a clip plus its runs, inspectable exactly like F-04 and F-07 (web)
- branches: Speaks again while the model is still replying — what happens is a design question, not a given, at this rung.
- states: *error* — provider fails mid-turn; the conversation shows the break at its moment. *empty* — he says nothing; dead time is recorded, and is itself a signal the supervisor will later read (EV-022).
- packets: derived at the packet station
- open: Is a live session the same object as a clip, or a distinct one that yields a clip? (data gate; EV-041 consequence)

**F-09  Talk to it out loud**
- actor: Owner
- trigger: The full stack exists and he wants a spoken conversation (EV-020).
- outcome: A voice conversation, recorded, with end-to-end latency attributed across every component.
- surface: web
- rung: 4
- steps:
  1. Starts a session (web)
  2. Speaks and is spoken to (web)
  3. Interrupts, and observes what the system does about it (web)
  4. Afterwards, inspects the whole call on one timeline: his audio, its audio, transcripts, turn decisions, model timings, cost (web)
- branches: The multi-LLM opener (EV-021) — the handoff point between the fast and the thinking model must be visible on the timeline, because an inaudible seam is exactly what needs verifying.
- states: *error* — TTS fails after the model has already committed to a reply. *conflict* — the second model wants to contradict what the first already spoke (EV-021's named design risk).
- packets: derived at the packet station
- open: none beyond the rungs below it.

**F-10  Greenlight a rung**
- actor: Owner
- trigger: An agent reports a rung complete.
- outcome: The rung is hardened and work on the next one may begin — or it is not, and it does not (EV-038).
- surface: **outside-web**
- rung: every rung
- steps:
  1. The agent hands over a rung that runs, with nothing left to set up (outside-web — the conversation)
  2. The owner drives the real surfaces himself: uploads, scrubs, runs, compares (web — F-02 .. F-09)
  3. He forms a judgment about whether it works and whether he trusts it (outside-web)
  4. He greenlights, or names what is wrong (outside-web)
  5. Only on greenlight does rung N+1 begin (outside-web)
- branches: Rejection — the rung is reworked and re-presented; there is no partial greenlight.
- states: *conflict* — the surface claims a rung works and driving it shows otherwise. This is the console failure verbatim (EV-009), and the flow exists to catch it.
- packets: derived at the packet station
- open: Is the greenlight recorded anywhere durable — a decision record per rung — or is it conversational? Recommend durable: with the bound waived (EV-030) and the bet closing on an owner call (EV-042), these greenlights are the only progress record the bet has.

**F-11  Read the supervisor**
- actor: Owner
- trigger: A conversation has happened and he wants to know what the watching system made of it (EV-022, EV-025, EV-026).
- outcome: A judgment about whether the supervisor's read of the call was correct.
- surface: web
- rung: above rung 4
- steps:
  1. Opens a conversation (web)
  2. Sees the supervisor's signals along the same timeline: dead time, emotional state, background speakers, ambient noise, inferred intention, predicted next utterance (web)
  3. Sees its sanity verdicts where they fired — "this is not something a human would do" — and any injection or hostility detections (web)
  4. Checks each against the audio at that moment (F-04) (web)
- branches: The supervisor acted rather than only observed (EV-025) — the intervention and its trigger appear on the timeline together.
- states: *empty* — the supervisor produced no signal for a span; shown as no signal, never as "all clear". This is the counters-must-name-what-they-witnessed rule applied to a system whose whole job is inference.
- packets: derived at the packet station
- open: Does the supervisor start record-only and gain the ability to act later, or act from the start? Record-only first is the reading most consistent with EV-029.

---

## Notes for the design gate

- Every flow above rung 1 renders against **the same timeline component** (F-04's). It is the
  product's one real screen; the others are ways into it. If the design gate draws it eleven
  times, the flows were rendered but the product was not understood.
- F-10 is `outside-web` and still gets a page: what the owner sees at handover, and what he sees
  after a rejection. That page is where a missing seam between agent work and owner judgment
  will show up.
- EV-013 licenses ugly. It does not license a surface that renders a plausible state instead of
  an observed one — see the `states:` lines above, which are mostly about exactly that.
