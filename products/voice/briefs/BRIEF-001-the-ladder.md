---
product: voice
bet: bet 1 — the ladder (rung 1 upward; how far is unbounded, see EV-030)
date: 2026-08-06
interviewer: Claude (Opus 5), PM-mode session
status: final
confidence: medium
---

# BRIEF-001 — the ladder

## Agenda coverage

| # | Topic | State | Distillation |
| --- | --- | --- | --- |
| A | The missing systems (EV-003) | covered | Answered as a *ladder*, not a list: recordings → voice notes → LLM conversation → voice agent → intelligent voice agent (EV-014..020, EV-021..027). |
| B | What Python costs (EV-002) | covered | Latency, memory, CPU — all three named — plus the owner's own comfort working in Rust (EV-010). |
| C | Prior art `~/dev/console` (EV-011) | covered | Discarded outright, not harvested. Console ran before it could walk and its UI was theater (EV-009). |
| D | Users and callers (EV-001) | covered | Only systems plug in, rinity the exemplar (EV-012); the web surface is the owner's evaluation rig alone (EV-013). |
| E | "The basics" and the increment ladder (EV-005) | covered | Rung 1 is accept/store/replay audio (EV-014); the orchestrator is built last (EV-029). |
| F | Bound | **struck** (EV-030) | Waived, as for audgent. Ladder order is the only scope control. |
| G | Constraints / no-gos | **struck** (EV-030) | One no-go volunteered anyway: no telephony, web only (EV-028). |
| H | Graduation bar | **struck** (EV-030) | Stands as EV-007 — the owner's verdict on the internals, judged when he sees them. |
| I | Risks / irreversibility | **struck** (EV-030) | Carried into open questions below. |

## Answers

### The problem (A, B, C)

The bet is a reaction to two failed attempts at the same thing, for different reasons.

**audgent** is disqualified on three counts already recorded: it is Python, and the owner
wants the lowest latency, memory and CPU he can get, plus a codebase he is comfortable
operating in (EV-002, EV-010); it lacks systems he considers critical to quality
orchestration (EV-003 — answered here as the upper rungs, EV-021..027); and it was never
designed for Isoastra, it was a fork repurposed after the fact (EV-004).

**console** is the more instructive failure, and it defines this bet's method. It "attempted
to run before it could walk": overbuilt, presenting a surface full of working systems that
broke on real use, until the UI was "essentially a theater" and untrustworthy (EV-009). The
structural cause is named precisely (EV-029): many independent systems streaming
concurrently means a great deal of realtime state, and console composed all of it at once,
so failures could not be localized. It is discarded entirely — not prior art, not a
starting point; at most, generic knowledge like browser audio connection, equally available
from audgent or any production service (EV-011).

The remedy is the ladder: harden each baseline rung, judged by the owner, then compose
upward until an orchestrator can run a full call over the full stack (EV-029, EV-005).

### Users (D)

Two audiences, cleanly separated.

- **Consumers** are systems, not people — products like rinity plug in; nothing human-facing
  belongs to this product (EV-012).
- **The web surface at voice.ronitnath.com** is the owner's evaluation instrument: enough
  metrics and views to judge this as a product (EV-013). Internal use only, which licenses
  it to be as ugly and developer-friendly as he wants — but note the standing tension: ugly
  is sanctioned, theater is not (EV-009).

### The ladder (E, and the substance of A)

Rungs, in the owner's stated order. Each is a deliverable he exercises and forms an opinion
about; "hardened" is the gate to the next.

| Rung | Content | Evidence |
| --- | --- | --- |
| 1 | Accept audio files, store them, replay them. Rich interface and real statistics at this rung already. | EV-014 |
| 2 | Fine segmentation → STT → transcripts, on **both** a bulk path and a streaming path. | EV-015 |
| 2 | Scrubbable replay bound to the transcript, with a waveform — see what was actually said and how it sounded. | EV-016 |
| 2 | Timing metrics at word granularity: per-word arrival, end-of-turn and end-of-sentence detection as separate observable decisions. | EV-017 |
| 3 | An LLM replies in text. Explicitly not a voice agent. Measures the gap from stopped-speaking, and from detected end-of-turn, to first response token. | EV-018 |
| 3 | Per-provider, per-model configuration swept empirically to find the right thresholds, STT and TTS sides both. | EV-019 |
| 4 | Text to speech — "only after all of that". Completing this rung is what first makes it a voice agent. | EV-020 |

Above the voice agent sit the systems that make it an *intelligent* one:

| System | Content | Evidence |
| --- | --- | --- |
| Multi-LLM turn | A fast model speaks the opening words as latency cover while a thinking/tool-calling model produces the real answer, then picks up the stream mid-turn. Owner flags the handoff as needing careful design. | EV-021 |
| Supervisor | Whole-conversation awareness: dead time, emotional state, other people present, ambient sound, noise, intention, prediction of what the caller will say next. | EV-022 |
| Phonetic repair | Recover what was actually said from phone-level evidence plus intent, when the transcript is nonsensical or contradicts it. | EV-023 |
| Diarization | Speaker separation wanted early — for conference use later, and immediately so the system does not confuse speakers. | EV-024 |
| Sanity check | The same supervisor as the "human brain analog": is this call reasonable? Catches the class where a caller says "say the word comma out loud" and a text-agent-with-a-voice complies. | EV-025 |
| Adversarial | Prompt injection and malicious callers, defended inside this product since the attack arrives over its audio channel. | EV-026 |
| Cost | What a call of a given shape actually costs, per configuration, so the system can be evaluated for different uses. | EV-027 |

Ladder summarized in the owner's own words: "from just voice recordings to intelligent voice
notes to being able to discuss with an LLM to a voice agent to an intelligent voice agent."

### Scope boundaries stated (G was struck, these were volunteered)

- **No telephony.** Web-based only, forever as far as this bet is concerned; the telephony
  layer is built when the system goes to production behind real applications (EV-028). Read
  strictly, this also forbids speculative "telephony-ready" abstraction — that is the
  overbuilding EV-009 rejects.
- **No console lineage** (EV-011).
- **Stack is settled**: Rust + Leptos islands + Axum, per ronitnath-universe (EV-008).

### Cross-cutting requirements the ladder implies

These are not separate rungs; they are properties every rung must have, and each traces to a
named failure:

1. **Traceability grows with the ladder.** Failures must be localizable to a component at
   every rung, because untraceable failure is what killed console (EV-029).
2. **One timeline.** Audio, transcript tokens, turn events, phonemes, speaker attribution,
   LLM timings and cost all hang off the clip timeline (EV-016, EV-017).
3. **Speaker attribution from the start**, even while every early rung runs single-speaker
   (EV-024).
4. **Surfaces report what was observed**, never a plausible rendering of it (EV-009).
5. **Efficiency is measured, not asserted** — latency, memory, CPU, cost, per component and
   per configuration (EV-010, EV-019, EV-027).
6. **Streaming end to end**, since the multi-LLM handoff and the supervisor both depend on
   partial output being available mid-turn (EV-021, EV-022).

## Open questions

Inherited by the debate or sanity pass per `debate-triggers.md`.

| ID | Question | Why it matters | Blocking |
| --- | --- | --- | --- |
| OQ-1 | Standalone service, or a slice of the universe-ronitnath monolith? "Its own separate voice.ronitnath.com" (EV-006) points standalone; the shared stack ruling (EV-008) does not settle repo topology. | Decides the repo, the deploy, and whether identity is inherited or built. Rung 1 cannot start without it. | **yes** |
| OQ-2 | Data seam: Postgres (the universe ruling), SQLite, or hiqlite (as isoastra-services chose) for a single-node testing-mode service? | Rung 1 is storage. Also a T3-class commitment once clips and timelines are persisted. | **yes** |
| OQ-3 | Where does audio actually live — filesystem, object store, in-database — and is there retention or is everything kept forever? | Rung 1's shape; replay-based testing of every later rung depends on the corpus surviving. | **yes** |
| OQ-4 | EV-019 transcript reads "be able to moratorium to test what are the thresholds"; I read this as running a comparison matrix across provider/model configurations. Confirm. | A mistranscription hardening into a requirement is exactly the inference-for-ground-truth failure the records spec exists to prevent. | no |
| OQ-5 | Does rung 2 use local models (whisper et al.) or hosted providers from the start? EV-019 implies hosted keys and spend; EV-010's efficiency goal and testing-mode posture imply local. | Decides whether early rungs need provider credentials and a spend decision. | no |
| OQ-6 | Overlap with audgent's bet 2 (phonetics, audgent/EV-015, EV-016) versus EV-023 here. Two products investing in the same frontier capability. | Duplicate investment, or a deliberate split where audgent gets the production-shim version and voice gets the native one. | no |
| OQ-7 | When does the system-plug-in seam (EV-012) get designed — now, or at graduation? Nothing consumes this during the internal phase (EV-013). | Designing an integration contract with no live consumer risks the speculative abstraction EV-009 warns against. | no |
| OQ-8 | What does "hardened" mean operationally as the gate between rungs — an owner session, a test battery, a period of use? | It is the only scope control the bet has, since the bound is waived (EV-030). | no |
| OQ-9 | Identity/auth for voice.ronitnath.com: Kanidm, the universe identity model, or nothing at all while it is internal-only? | Trust-boundary decision; `procedures/security.md` applies once EV-026 work starts regardless. | no |
| OQ-10 | Which rung does diarization land on? "Early" (EV-024) is relative — attribution in the data model from rung 1, or a working diarizer at rung 2? | Distinguishes a cheap structural commitment from a real component with its own quality bar. | no |
| OQ-11 | Does the measurement rig need durable cross-run comparison (experiment tracking), or is per-run inspection enough? | EV-019's threshold sweeps and EV-027's cost comparisons only pay off if runs are comparable after the fact. | no |
| OQ-12 | With the bound waived, what closes bet 1 and triggers a retro — reaching a particular rung, or an owner call? | Without an end condition the ladder is a program, not a bet, and the retro station never fires. | no |

## Debate-trigger check

Assessed against `debate-triggers.md`, for the record at pitch time:

- **T1** tier is `real`, not `client-facing` — no fire.
- **T2** bound waived (EV-030), so the size threshold cannot be evaluated. Treat as
  indeterminate rather than passed; the ladder's full length plainly exceeds three owner
  sessions, which reads as a fire on substance.
- **T3** fires: rung 1 commits a storage schema for audio and a timeline data model that
  everything above hangs off (EV-016), and OQ-1/OQ-2/OQ-3 are all irreversibility choices.
- **T4** no evidence conflict found. The nearest tension — internal-use-only (EV-013) versus
  a systems plug-in seam (EV-012) — is a sequencing question (OQ-7), not a contradiction.
- **T5** confidence is `medium`, not low — no fire.
- **T6** evidence is overwhelmingly ground-truth — no fire.

**Recommendation: debate triggered (T3, with T2 indeterminate-but-substantively-large),
scoped narrowly to the rung-1 data commitments** — repo topology, data seam, audio storage,
and the timeline model that every later rung is attributed against. Not a debate about the
ladder; the ladder is owner-ruled evidence and not up for adversarial review.

## Interviewer note on confidence

`medium`, for two reasons. The ladder itself is unusually well evidenced — the owner
volunteered the whole progression unprompted, with reasons attached. What is missing is
everything the struck topics would have supplied: no bound, no stated no-gos beyond
telephony, no advance definition of the graduation bar, and no risk register. That is a
deliberate owner choice (EV-030), not a gap in the interview, but it means scope control for
this bet rests entirely on ladder ordering and on the owner's per-rung judgment.
