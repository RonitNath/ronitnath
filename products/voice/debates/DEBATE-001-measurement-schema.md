# DEBATE-001 — the measurement/timeline schema

Generated from `DEBATE-001-measurement-schema.yaml`. Panel: `DEBATE-001-lenses/panel.md`.

- **Trigger**: T3, relocated by EV-041 from the storage choice to the measurement schema.
- **Panel**: 3 axes, 6 lenses, one OS process each, launched concurrently and blind to each other.
- **Lens model**: `gpt-5.6-sol` via pi/codex — decorrelated family (heuristics rule 5).
- **Judge**: Claude (Opus 5), who authored FLOWS-001 and BRIEF-001. **Compromised judge**
  (rule 6). This memo is an input the owner reviews, not a verdict.
- **Grounding**: 51 claims, **zero** with inference-only provenance. The round is not void
  under rule 2.
- Every lens returned at least one staked claim; no lens returned a clean bill of health.

---

## Options considered

### Option 1 — nothing (rung1-builder, C-10..C-16)

One table, `audio_clips`, with immutability triggers. No measurement model, no run identity,
no comparability predicate, no reserved extension points, because empty extension points are
themselves commitments that code starts depending on. D1–D4 are answered by refusing to
answer them until a rung exists that needs them.

### Option 2 — an append-only observation ledger (archivist, instrument, notary, realtime; C-1, C-17, C-26, C-43)

One `observations` stream with a fixed typed envelope and versioned signal payload definitions;
typed per-signal tables demoted to disposable projections. Four lenses with four different
mandates proposed nearly the same envelope: observation id, run id, signal-definition id,
producer invocation, subject/speaker, media coordinates, occurrence coordinates, causal parents,
status, payload, provenance edges.

### Option 3 — recipes, not results (replayer, C-34, C-35, C-42)

Store only what is needed to *re-execute*: clip content hashes, canonical immutable
configurations, replay recipes, comparison contracts. Every segment, transcript, word, timing
and cost is regenerated on demand and discarded. A successful implementation could delete all
generated results without losing measurement capability.

---

## Rejected, and why

**Option 3 is rejected** (C-34, C-42 → resolved). EV-035 and EV-033 defeat it jointly: hosted
providers are nondeterministic and get deprecated, so a derivation is not a function of audio
plus configuration, and a corpus kept forever cannot be measured against models that no longer
exist. C-32 adds a second kill — re-execution on read puts model work beside live audio, which
EV-010 forbids. Worth noting how this rejection was produced: the replayer lens was explicitly
**forbidden** from conceding provider nondeterminism, so it argued the strongest available case
and lost anyway. That is the panel design working, not a lens failing.

**Option 3's identity discipline survives** (C-35, C-36 → resolved). Content-hash clip identity
and a canonical configuration snapshot that closes over *every* input capable of changing what a
measurement means — provider, model, thresholds, adapter and engine digests, runtime profile,
instrumentation and comparator versions, tariff inputs — is a better statement of the recipe
requirement than the notarizing lenses managed. Adopted as part of the ledger, not instead of it.

**Option 1 is rejected on its own criterion** (C-13, C-16 → resolved). The lens argues nothing
unexercised may be built. But rung 1 *does* derive signals: EV-014 says statistics are measured
at rung 1, and F-02 computes a waveform. The envelope therefore has a live producer at rung 1
and is not speculative. Recorded honestly: this lens was not given FLOWS-001 (see the asymmetry
table) and read EV-014 as storage-only, which is the asymmetry doing its job rather than a flaw.

**Option 1's strongest point survives and binds** (C-16). Empty extension points are still
commitments. So the rung-1 migration creates only what rung 1 produces — clips, plus waveform
and clip-statistic observations — with **no reserved columns** for transcripts, turns, costs,
emotion or anything else. Later signal families arrive as new immutable definitions, never as
columns waiting to be filled.

**No comparability predicate at rung 1** (C-14 → resolved). Correct, and it does not survive
rung 2. No comparison exists until runs exist; the contract table arrives with the first run.

---

## Recommendation

The surviving cluster, in the order it should be built:

1. **One append-only `observations` stream is the authoritative timeline** (C-1, C-17, C-26,
   C-43). Per-signal typed tables are read-side projections, regenerable, never the source of
   truth. UPDATE and DELETE rejected by SQLite triggers, not just by convention (C-43, C-12).

2. **Two time coordinates, never collapsed into one** (C-5, C-19, C-38, C-47). Integer sample
   positions into the immutable clip for media truth; monotonic nanoseconds from run start for
   arrival and processing. Wall time only for correlating with hosted-provider records. Clock
   domain and precision are recorded fields, so a later comparison cannot silently treat
   provider time, wall time and process-monotonic time as the same clock.

3. **Runs are immutable manifests** (C-4, C-22, C-46, C-36). Clip content hash and span,
   canonical configuration snapshot and digest, provider and model identifiers as requested
   *and* as resolved by the provider, component build digests, clock domains, measurement
   protocol version. Facts learned after start — provider request ids, billing reports — append
   as observations rather than patching the manifest (C-46). Failed, rate-limited and timed-out
   executions stay runs with manifests (C-4).

4. **Every derived signal is persisted at production time; regeneration creates a new run**
   linked to its sources, never a replacement (C-8, C-20, C-32, C-48).

5. **Comparability is a versioned contract held as data** (C-6, C-23, C-31, C-39, C-49),
   evaluated per metric by one canonical comparator, not by whole-manifest equality — since
   provider and model are usually the dimensions a matrix intends to vary. Each contract names
   controlled dimensions, permitted varying dimensions, required signal-definition hashes,
   units and clock requirements. Comparison attempts are themselves appended with their
   comparator version, verdict and machine-readable reasons.
   *Caveat recorded*: all five lenses read EV-041, which states the concern, so this convergence
   is partly evidence-driven rather than independent. It survives because each reached it by a
   different route.

6. **Refuse, never degrade — per metric** (C-7, C-24, C-40, C-50). Two runs may support a
   transcript-disagreement comparison while refusing an arrival-latency one. The refusal lives
   in the comparator and the API, not in presentation code another caller could bypass. This is
   EV-009 expressed as a database rule.

7. **Absence is a recorded fact** (C-21, C-51). Expected-but-absent output, provider silence,
   dropped telemetry and unfinished work are four different things and must never share a
   representation. Explicit stream-open/close, sequence numbers and coverage observations make
   a hole in the evidence distinguishable from an observed silence.

8. **Nothing on the audio path touches SQLite** (C-27). A single recorder thread owns the write
   connection and batches inserts; producers hand off already-owned buffers to a bounded
   preallocated queue. Signal-definition ids and manifests resolve before audio starts, so
   emission performs no lookup. This is why the envelope must be fixed early — the hot path
   cannot afford to branch on schema.

9. **Speaker/subject is in the envelope from rung 1** (C-29, C-44, C-9), optional and unused
   until diarization lands at rung 2. EV-024 and EV-040 make a single-speaker assumption a
   foundation-level error, and this is the cheapest possible way to not make it.

---

## What would change it

**Deferred as assumption — queue overflow policy** (C-28 rebuts C-21, the one genuine Axis-2
collision). Assume bounded queue with drop-and-record: the live path never blocks, and each
drop appends an explicit gap observation with counts, which satisfies the requirement that
absence be visible. Assumed further: lifecycle and terminal observations — start, failure,
completion, provider request identity — are never droppable; only high-volume payload
observations are. Cheap to reverse: it is a queue policy, not a schema shape.

**Unresolved — is a waveform an observation or a storage fact?** (C-15). rung1-builder says the
hash, byte count and waveform are storage-integrity facts, not observations against a timeline.
The judge leans observation — it is derived, it has a producer version, and F-04 renders it —
but this decides whether rung 1 ships the ledger at all, and EV-038 reserves rung scope to the
owner.

**Would reopen the recommendation**: a decision to run local models rather than hosted ones
(reversing EV-035) removes the nondeterminism argument and makes Option 3 live again. So would
evidence that the corpus is not in fact kept forever.

---

## Escalated — one consolidated round for the owner

**E-1 (C-9, C-2): how much of the envelope ships at rung 1?**

Blocking and undecidable by agents, because it sits between two of your own rulings and the
panel cannot rank them. The full envelope — content-addressed signal-definition registry, named
clock domains with declared precision, causal parent edges, explicit absence and coverage
observations — is either the thing that makes a forever-corpus worth keeping (EV-041) or the
overbuilt foundation that killed console (EV-009).

The three positions the round produced, stated as they would actually be built:

| | Rung 1 creates | Cost if wrong |
| --- | --- | --- |
| **Minimal** (C-13, C-16) | `audio_clips` only. Waveform and statistics computed on read, stored nowhere. | Rung 2 rewrites the foundation, and rung-1 measurements are not comparable to anything later — the churn EV-041 warns about. |
| **Envelope-now** (judge's lean) | `audio_clips`, `runs`, `observations`, `signal_definitions`. Rung 1's only signals are waveform and clip statistics. No registry compatibility edges, no coverage observations, no comparison contracts. | Roughly one week of structure that rung 1 does not strictly need, and four table names that are hard to walk back once code depends on them. |
| **Full envelope** (C-9, C-2) | All of the above plus the immutable definition registry with compatibility edges, clock-domain calibration records, and coverage observations. | The console failure repeating — machinery built before a producer exercises it, presenting as a working system. |

Judge's recommendation is **envelope-now**, on the ground that rung 1 has real producers
(waveform, duration, sample rate) so the envelope is exercised rather than speculative, and
that the pieces held back — registry compatibility edges, coverage observations, comparison
contracts — are exactly the pieces with no rung-1 producer. But this is the compromised judge
recommending a middle option, and it is your call under EV-038.
