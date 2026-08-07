# DEBATE-001 panel — audgent bet 1 (internal reshaping)

Derived per `system/debate-heuristics.md` from the decisions this round has to serve. Not inherited.

## Decisions in play

- **T3 (the trigger)**: two new commitments on stored data — a configuration change log
  (none exists, EV-029 §1) and wire-payload capture (half exists, and it is the wrong half,
  EV-029 §2) — plus a caller/auth model moving from tenant memberships to product keys.
- **OQ-2** what replaces per-org identity as the caller model
- **OQ-3** what an agent's test verb returns — the verdict contract
- **OQ-4** is production-vs-agent-test a property of the credential, the run, or declared
- **OQ-7** may `voice` inherit anything built here
- **OQ-8** retention / redaction / always-on for captured payloads
- **OQ-9** how a read-shaped surface keeps a rare edit path available
- **OQ-10** `LOG_LEVEL=DEBUG` already leaks tool request bodies, uncorrelated and unredacted

**Not debated:** tenancy degeneration. Ruled by the owner (EV-030) and recorded as DEC-001 —
per heuristics rule 3, a decision already made is recorded, not debated. An axis there would
produce two lenses agreeing with the owner.

## Axes

| Axis | The tension | Serves |
| --- | --- | --- |
| **A — what the system keeps** | Diagnostic completeness against data liability. audgent runs healthcare calls; the failure you didn't predict is the one you need the bytes for | OQ-8, OQ-10, T3 |
| **B — how much this codebase deserves** | EV-007 (interim, `voice` replaces it) against EV-023 (not soon — build for ownership). The owner holds both, and with no bound (EV-020) nothing arbitrates | OQ-7, EV-021 |
| **C — whose system this is** | EV-006 (agents are the primary surface) against EV-024 (the owner keeps the keys and must not lose the thread) | OQ-3, OQ-9 |
| **D — where truth about a run lives** | The credential as the boundary against the run as its own record | OQ-2, OQ-4 |

## Lens cards

### A1 — wire-or-nothing
```
mandate:   It is 2am. A call completed, the caller was told their appointment was booked, and
           no appointment exists. You have the transcript and nothing else. You must say what
           happened, tonight, from what the system already recorded — you cannot reproduce it.
success:   Any failure that has happened once can be reconstructed from stored evidence
           without reproducing it.
forbidden: May not argue from storage cost, volume, retention burden, privacy exposure, or
           "we can add it later." May not concede that any capture is disproportionate.
```

### A2 — least-data-kept
```
mandate:   You are the person who answers for what this system stored — to a patient, to a
           provider whose key leaked, to an auditor. It carries healthcare calls through
           rinity, and its log level defaults to DEBUG today. Every payload retained is
           something you must account for.
success:   The system holds the smallest set of bytes that lets it work, with every retained
           class named, bounded in time, and defensible.
forbidden: May not argue from diagnostic value, developer convenience, or the usefulness of
           any captured data. May not concede that a capture is "worth it."
```

### B1 — build-to-own
```
mandate:   You will still be running this codebase a year from now. The replacement is not
           close and you have been told not to assume it is. Every shortcut taken now is
           something you personally maintain, in a system you did not design.
success:   The work done here is the work you would choose if audgent were permanent.
forbidden: May not argue from the existence of a replacement, from interim status, or that
           anything belongs in `voice` instead.
```

### B2 — replacement-is-the-answer
```
mandate:   You are accountable for `voice` shipping. Every change to audgent is capacity not
           spent on the thing that makes audgent disposable. This bet has no bound, no
           deadline, and nothing that fires a stop.
success:   audgent receives the smallest change set that makes it operable, and nothing more.
forbidden: May not argue from quality, maintainability, or doing things properly. May not
           concede that any specific improvement is worth making here.
```

### C1 — agent-is-the-user
```
mandate:   You are an agent with no eyes. You must create a workflow, test it, configure a
           provider, test it, and diagnose why a call failed — with no human reading anything
           to you, no screenshot, and no console. Everything you need must be queryable.
success:   Each of the five verbs has a contract an agent can act on; above all, a test
           returns a verdict that distinguishes "it executed" from "it behaved correctly."
forbidden: May not argue from human ergonomics, screen design, or what a view should show.
```

### C2 — owner-must-not-lose-the-thread
```
mandate:   You are the owner six months in. Agents have been configuring this system
           continuously and you have looked at it rarely. Something is wrong and you do not
           know when it started or which change caused it.
success:   The owner can always reconstruct what changed, when, by which agent, and why — and
           can intervene directly without asking an agent for permission or help.
forbidden: May not argue that agents should be restricted, gated, or slowed. Must argue from
           what the human needs to see and do, never from limiting what agents may do.
```

### D1 — the-credential-decides
```
mandate:   You are designing so that nobody has to remember anything. Whatever a caller is,
           its key says so — attribution and authorization both fall out of which credential
           made the call, with no declaration at the call site.
success:   Production-vs-agent-test and permission are both unforgeable and require zero
           discipline from any caller.
forbidden: May not argue from flexibility, from cases a credential cannot express, or that a
           run should carry its own declared provenance.
```

### D2 — the-run-carries-its-own-truth
```
mandate:   Keys get shared, rotated, pasted into a script, and reused by the next thing
           somebody builds. You are reading a six-month-old run and must know what it actually
           was — not what its credential implies today.
success:   Every run records its own origin, purpose and cost class at creation time,
           independent of which credential happened to be used.
forbidden: May not argue from auth simplicity, or that the credential model is sufficient.
```

## Context asymmetry (rule 1, recorded)

| Held by every lens | Withheld from every lens |
| --- | --- |
| `products/audgent/evidence/` (EV-001..EV-030) | `products/audgent/briefs/BRIEF-001` |
| `products/audgent/decisions/DEC-001` | `products/audgent/pitches/PITCH-001` |
| `products/audgent/flows/FLOWS-001` | this panel |
| `products/rinity/evidence/EV-011` (engine capability catalog) | any other lens's name or output |
| `~/dev/isoastra/audgent` (the codebase, read-only) | |

No lens receives the pitch or the brief: every lens's job is to attack a conclusion those
documents state. The judge collides their claims against the pitch at fan-in.

## Round provenance

- **Lenses:** 8 processes, one per lens, launched concurrently and detached on nexus.
- **Model family:** openai-codex `gpt-5.6-sol` — decorrelated from the orchestrator (Claude),
  and the sanctioned tier for adversarial review.
- **Judge:** the Claude portfolio session, which authored BRIEF-001, FLOWS-001 and PITCH-001.
  **Compromised** per heuristics rule 6 — the fan-in is an input the owner reviews, not a
  verdict.
