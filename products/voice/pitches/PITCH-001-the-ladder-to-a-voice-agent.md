---
id: PITCH-001
product: voice
bet: bet 1
date: 2026-08-06
status: frozen
baseline: pitch/voice-1
flows: FLOWS-001
brief: BRIEF-001
debate: triggered (T3: measurement schema) → memo DEBATE-001-measurement-schema
---

# PITCH-001 — the ladder to a voice agent

## Problem

We run voice agents on audgent, and audgent is wrong on three counts that no amount of
improving it fixes. It is Python, when what we want is the lowest latency, memory and CPU
available and a codebase the owner is comfortable operating in (EV-002, EV-010). It lacks the
systems that make voice orchestration good rather than merely working (EV-003). And it was never
designed for this — it is a fork of someone else's product, repurposed after the fact (EV-004).

The previous attempt at a replacement, `~/dev/console`, failed differently and more usefully. It
composed every realtime system at once, so when things broke the failure could not be localized;
it presented a surface full of working systems that broke on contact; and the UI became "essentially
a theater" the owner could not trust (EV-009, EV-029). That failure defines this bet's method.

## Appetite

**Waived** (EV-030, SYS-DEC-001 satisfied by written waiver — no agent may substitute an estimate).
Scope control is the ladder: one rung at a time, each gated by an owner greenlight (EV-038), and
the bet closes when the owner says it closes (EV-042).

## Solution

A greenfield Rust service at voice.ronitnath.com — standalone, SQLite, audio on the filesystem
kept forever, behind Kanidm (DEC-001) — built one rung at a time. No rung N+1 work begins before
rung N is greenlit.

| Rung | What exists when it is done |
| --- | --- |
| **1** | Audio goes in, is stored permanently, and comes back out. Waveform, duration, sample rate and channels are measured and recorded as observations. The four-table measurement foundation exists (DEC-002, EV-043). |
| **2** | Fine segmentation, hosted STT on both a bulk and a streaming path, and a working diarizer *before* any model sees a transcript. A scrubbable replay bound to the transcript with the waveform. Per-word arrival times; end-of-turn and end-of-sentence as separate observable decisions. Runs, configurations and comparisons. |
| **3** | An LLM replies in text. Not a voice agent. Two latency baselines become real: speech-stop → first token, and detected end-of-turn → first token. Provider/model threshold sweeps. |
| **4** | Text to speech. Completing this rung is what first makes it a voice agent. |

**The measurement model is the load-bearing commitment**, and it is settled: an append-only
observation ledger with versioned signal definitions, two time coordinates that are never
collapsed, immutable run manifests, derived signals persisted at production time, and
comparability as versioned data evaluated per metric that refuses rather than degrades
(DEC-002). Everything the ladder later produces attaches to that timeline.

The web surface is the owner's evaluation instrument and nobody else's (EV-013). It may be as
ugly and developer-friendly as he likes. It may not be theater: every surface reports what was
observed, and absence renders as absence.

## Flows

Full detail in `FLOWS-001-the-ladder.md`, frozen with this pitch.

| ID | Actor | Outcome | Rung |
| --- | --- | --- | --- |
| F-01 | Owner | An authenticated session; the service knows who is acting. | 1 |
| F-02 | Owner | A clip is stored permanently, with its statistics measured. | 1 |
| F-03 | Owner | A clip recorded in the browser, indistinguishable from an uploaded one. | 1 |
| F-04 | Owner | A judgment about transcript quality against the audio itself. | 2 |
| F-05 | Owner | A run exists, attributed to its exact configuration and code version. | 2 |
| F-06 | Owner | A defensible judgment about which configuration is better. | 2 |
| F-07 | Owner | A failure localized to one component, from stored data alone. | 2 |
| F-08 | Owner | A text conversation, recorded, with both latency baselines measured. | 3 |
| F-09 | Owner | A spoken conversation, with latency attributed across every component. | 4 |
| F-10 | Owner | A rung is hardened and the next may begin — or it is not. (`outside-web`) | every |
| F-11 | Owner | A judgment about whether the supervisor read the call correctly. | bet 2 |

Named non-goals with zero flows, deliberately: consuming systems (there is no consumer —
EV-037) and end callers (they are subjects inside recordings, never users — EV-012).

## Rabbit holes

- **The measurement schema.** Debated (DEBATE-001) precisely because this is where the bet could
  disappear. Settled in DEC-002; build it, do not relitigate it.
- **Bulk versus streaming transcripts on one timeline.** How two transcripts of one clip are
  held — two transcripts, or one with revisions — is open and belongs to the data gate (F-04).
- **The registry compatibility machinery.** Deliberately deferred until a second signal family
  exists (EV-043). If rung 2 finds the envelope needs a column that every existing observation
  must backfill, that deferral was wrong and becomes urgent — the reopener named in DEC-002.
- **Provider adapters as a breadth feature.** They exist as instruments under test, not as a
  parity surface. That is the audgent trap (audgent/EV-003).
- **The multi-LLM handoff** (EV-021). The owner flagged the design risk himself. It is bet 2, and
  it needs the join point visible on the timeline before it is trusted.

## No-gos

- **No telephony.** Web only. Read strictly: also no speculative "telephony-ready" abstraction,
  because that is the overbuilding that killed console (EV-028, EV-009).
- **No console lineage.** Not a port, not a reference, not a starting schema (EV-011).
- **No multi-tenancy, no consumer API, no integration seam.** There is no consumer; the seam is
  designed at graduation against a real one (EV-037).
- **No reserved columns** for signals that do not exist yet (DEC-002, DEBATE-001 C-16).
- **No TTS before rung 4**, however much it would make a demo feel finished (EV-020).
- **No rung N+1 work before rung N is greenlit** (EV-038).

## Bet boundary

This pitch covers rungs 1–4, ending at a working voice agent. The intelligence layer — multi-LLM
latency cover, the conversation supervisor and sanity check, phonetic repair, injection defense
(EV-021 .. EV-027) — is **bet 2**, and F-11 belongs to it. That line is the judge's proposal, not
an owner ruling; EV-042 puts the boundary in the owner's hands and he may move it.

## Debate

```
debate: triggered (T3: measurement schema) → memo DEBATE-001-measurement-schema
```

Six lenses on three derived axes, one detached process each, `gpt-5.6-sol` for family
decorrelation. 51 claims, zero inference-only, nothing left unresolved. One escalation, resolved
by the owner as EV-043. Judge was compromised (authored the flows and brief) and said so.
