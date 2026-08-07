# DEBATE-001 panel — the measurement/timeline schema

Trigger: **T3** (irreversible commitment), relocated by EV-041 from the storage choice to the
measurement schema. The storage seam is already ruled (DEC-001) and is cheap to reverse; the
timeline model is not, because a corpus kept forever (EV-033) is only worth keeping if runs
recorded across it stay comparable.

## Decisions this round serves

| # | Decision |
| --- | --- |
| D1 | How rigid or extensible the timeline model is — typed per-signal tables against a generic annotation stream — given it must absorb signals that do not exist yet (EV-021..027) without reshaping the ones that do. |
| D2 | What makes two runs comparable, where that predicate lives, and whether the system refuses a comparison or degrades it (EV-041, F-06). |
| D3 | Whether derived signals are stored as observations or regenerated from audio + configuration (EV-033, EV-035). |
| D4 | How much of the upper-rung signal shape must be designed at rung 1, against EV-009's anti-overbuilding ruling. |

## Axes

**Axis 1 — durability of meaning against smallness of commitment** (D1, D4).
Neither end is correct: a schema built to survive four generations of signals is the
overbuilding that killed console; a schema built only for rung 1 is the schema churn EV-041
names as the threat to measurement.

**Axis 2 — completeness of the record against cost in the hot path** (D2, and EV-010).
Every measurement is a write inside a realtime audio path whose stated goal is the lowest
latency, memory and CPU available.

**Axis 3 — recompute against notarize** (D3).
Either the audio plus the configuration is the truth and everything else is derivable, or the
observation recorded at the moment it happened is the only truth there will ever be.

## Lenses

Six lenses, three opposed pairs, one process each.

```
archivist
mandate:    It is 2029. You are answering "did latency get better?" across four generations of
            this system's schema, using runs recorded in 2026 by code that no longer exists.
success:    Every run ever recorded is still interpretable and still comparable to a new one.
forbidden:  May not argue from build cost, from simplicity, or from "add it when we need it".
```
```
rung1-builder
mandate:    It is rung 1. The system accepts audio files, stores them, replays them. Nothing
            else exists and nothing else is greenlit. Anything you build past that is the move
            that killed console.
success:    The smallest model that serves rung 1 and is cheap to throw away if it is wrong.
forbidden:  May not argue from signals that do not exist yet, nor from preserving comparability
            across future schema generations.
```
```
instrument
mandate:    A call had an unexplained 400ms gap. The call is over, the audio is on disk, the
            provider is a black box, and you cannot reproduce it. Find the component responsible
            from stored data alone.
success:    Any failure is localizable after the fact, from what was written down.
forbidden:  May not argue from write cost, storage cost, or runtime overhead.
```
```
realtime
mandate:    You are inside the audio path. Every write, lock, allocation and await you add is
            latency a caller hears and memory the box holds.
success:    The hot path does the minimum; measurement never blocks it or perturbs what it measures.
forbidden:  May not argue from "we would lose visibility", nor from the owner's evaluation needs.
```
```
replayer
mandate:    Disk fills, derived data rots, and every stored derivation is a copy that can
            disagree with its source. The audio and the configuration are the truth; the rest is
            a function of them.
success:    The minimum is stored; the maximum is regenerated.
forbidden:  May not concede that hosted-provider nondeterminism forces results to be stored.
            That concession is the collapse of this lens.
```
```
notary
mandate:    A hosted model you called in August is deprecated in November. The number you wrote
            down is the only evidence that run ever happened, and it can never be produced again.
success:    Every observation is recorded immutably at the moment it occurs, never recomputed.
forbidden:  May not argue from storage cost, nor from the possibility of re-running anything.
```

## Asymmetric context (rule 1, recorded)

| Lens | Reads |
| --- | --- |
| archivist, instrument, notary | `evidence/*`, `decisions/DEC-001`, `flows/FLOWS-001` |
| rung1-builder, realtime, replayer | `evidence/*`, `decisions/DEC-001` — **no flows** |
| all | `system/claim-schema.md` |
| none | `briefs/BRIEF-001` — withheld from every lens; it is the orchestrator's own synthesis and would leak its framing into all six |

## Round provenance

- Orchestrator/judge: Claude (Opus 5) — **also the author of FLOWS-001 and BRIEF-001, therefore
  a compromised judge** (heuristics rule 6). The fan-in is an input the owner reviews, not a verdict.
- Lenses: `gpt-5.6-sol` via pi/openai-codex — different model family from the orchestrator
  (heuristics rule 5), justified by T3 stakes.
- Launched concurrently and detached, one process per lens, blind to each other.
