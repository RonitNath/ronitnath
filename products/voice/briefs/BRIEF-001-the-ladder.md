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

**All twelve resolved by owner ruling, 2026-08-06** (same session, immediately after the
brief). Kept here with their resolutions rather than deleted — the question is why the
answer binds.

| ID | Question | Resolution | Evidence |
| --- | --- | --- | --- |
| OQ-1 | Standalone service or a slice of the universe monolith? | Standalone service, own repo. | EV-031, DEC-001 |
| OQ-2 | Data seam — Postgres, SQLite, hiqlite? | SQLite. | EV-032, DEC-001 |
| OQ-3 | Where does audio live; is anything deleted? | Filesystem; kept forever. | EV-033, DEC-001 |
| OQ-4 | Is the garbled span in EV-019 a provider comparison matrix? | Yes — reading confirmed. | EV-034 |
| OQ-5 | Local models or hosted providers at rung 2? | Hosted. Credentials and real spend in scope from rung 2. | EV-035 |
| OQ-6 | Phonetics duplicated across audgent and voice? | Deliberate duplicate; neither waits on the other. | EV-036 |
| OQ-7 | When is the system plug-in seam designed? | Not now — there is no consumer; standalone internal product. Seam is designed at graduation, against a real consumer. | EV-037 |
| OQ-8 | What does "hardened" mean between rungs? | Owner greenlight. No rung N+1 work before rung N is greenlit. | EV-038 |
| OQ-9 | Identity/auth? | Kanidm, from rung 1. | EV-039, DEC-001 |
| OQ-10 | Which rung does diarization land on? | A working diarizer at rung 2 — before the text agent ever sees a transcript. | EV-040 |
| OQ-11 | Durable cross-run comparison needed? | Yes, and the named threat is schema churn breaking comparability. | EV-041 |
| OQ-12 | What closes bet 1? | Owner call. Not a rung count. | EV-042 |

Two of these reshape the ladder rather than merely answering a question:

- **OQ-10** moves diarization from a schema commitment to a rung-2 deliverable with its own
  greenlight, ahead of the LLM turn.
- **OQ-11** identifies the actual irreversibility in this bet. It is not the storage choice
  (SQLite on a filesystem is easy to walk back); it is the **measurement schema**, because a
  corpus kept forever (EV-033) is only worth keeping if runs stay comparable across it.

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

**Recommendation, revised after the OQ round: debate triggered (T3), scoped to one subject —
the measurement/timeline schema.** Repo topology, data seam and audio storage were T3
candidates when the brief was written; the owner has since ruled them (DEC-001) and they are
cheap to reverse besides. What remains is the commitment EV-041 names: the timeline model
every signal is attributed against — audio, segments, transcript tokens, word arrival times,
turn and sentence boundaries, speaker attribution, phonemes, LLM timings, cost — which must
absorb signals that do not exist yet (EV-021..027) without reshaping the ones that do, or the
permanent corpus stops being comparable and stops being worth keeping. Not a debate about the
ladder; the ladder is owner-ruled evidence and not up for adversarial review.

## Interviewer note on confidence

`medium`, for two reasons. The ladder itself is unusually well evidenced — the owner
volunteered the whole progression unprompted, with reasons attached. What is missing is
everything the struck topics would have supplied: no bound, no stated no-gos beyond
telephony, no advance definition of the graduation bar, and no risk register. That is a
deliberate owner choice (EV-030), not a gap in the interview, but it means scope control for
this bet rests entirely on ladder ordering and on the owner's per-rung judgment.
