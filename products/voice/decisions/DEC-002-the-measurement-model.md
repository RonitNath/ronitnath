---
id: DEC-002
date: 2026-08-06
source: DEBATE-001 (T3, six lenses) plus the owner's ruling on its one escalation (EV-043)
---
Chose **an append-only observation ledger with versioned signal definitions, persisted at
production time** over **regenerating derived signals from audio plus configuration**, and over
**deferring the measurement model until a rung needs it**, because hosted providers are
nondeterministic and get deprecated (EV-035) while the corpus is kept forever (EV-033) — so a
derivation is not a function of its inputs and cannot be recovered later — and because rung 1
already has real signal producers (EV-014, F-02), which makes the envelope exercised rather than
speculative.

| Rule | The build must now |
| --- | --- |
| One authoritative timeline | `observations`, append-only. UPDATE and DELETE rejected by SQLite triggers, not by convention. Per-signal typed tables are read-side projections, regenerable from the ledger, never the source of truth. |
| Two time coordinates, never collapsed | Integer sample positions into the immutable clip for media truth; monotonic nanoseconds from run start for arrival and processing. Wall clock only to correlate with hosted-provider records. Clock domain and precision are recorded fields. |
| Runs are immutable manifests | Clip content hash and span, canonical configuration snapshot and digest, provider/model as requested *and* as resolved, component build digests, clock domains, measurement-protocol version. Facts learned after start (provider request ids, billing) append as observations; the manifest is never patched. |
| Failed runs are runs | Rejected, rate-limited and timed-out executions keep their manifest and terminal observations, with their cost. They do not disappear. |
| Derived signals persist at production time | Regeneration creates a **new** run linked to its sources. It never replaces a historical result. |
| Comparability is data, not code | A versioned comparison contract naming controlled dimensions, permitted varying dimensions, required signal-definition hashes, units and clock requirements. Evaluated per metric by one canonical comparator — never whole-manifest equality, since provider and model are usually what a matrix intends to vary. Arrives at rung 2 with the first run, not at rung 1. |
| Refuse, never degrade — per metric | Two runs may support a transcript-disagreement comparison while refusing an arrival-latency one. The refusal lives in the comparator and the API, not in presentation code a caller could bypass. |
| Absence is a recorded fact | Expected-but-absent output, provider silence, dropped telemetry and unfinished work are four distinguishable representations. Never one missing row meaning all four. |
| Nothing on the audio path touches SQLite | One recorder thread owns the write connection and batches inserts; producers hand off owned buffers to a bounded preallocated queue. Definition ids and manifests resolve before audio starts. Overflow drops and appends an explicit gap observation with counts — but lifecycle and terminal observations are never droppable. |
| Subject in the envelope from rung 1 | Optional, unused until diarization lands at rung 2. EV-024 and EV-040 make a single-speaker assumption a foundation-level error. |
| Rung 1 creates exactly four tables | `audio_clips`, `runs`, `signal_definitions`, `observations`. Signals: waveform, duration, sample rate, channels. **No reserved columns** for signals that do not exist (EV-043, and DEBATE-001 C-16). |
| Held back until a producer exists | Registry compatibility edges, coverage/gap observations, comparison contracts. |

**What was given up knowingly**: the recompute position's real prize — a database small enough
to stay an execution catalog rather than becoming a second corpus, where no stored derivation
can ever disagree with its source. And the minimal position's prize — four table names that
nothing depends on yet, so rung 2 could choose freely. We are paying roughly a week of structure
rung 1 does not strictly need, and committing names before rung 2 proves them right (EV-043).

**What would change this**: a decision to run local models instead of hosted ones (reversing
EV-035) removes the nondeterminism argument and makes recompute live again. So would evidence
that the corpus is not in fact kept forever (reversing EV-033). Narrower reopener: if rung 2
finds the envelope needs a fifth column that every existing observation must backfill, the
envelope was wrong and the deferred registry-compatibility machinery becomes urgent rather than
speculative.
