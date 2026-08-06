# portfolio — product operating system

Canonical product graph for all products. Solo operator, agent workers. Four kinds of truth, kept separate, joined by stable IDs:

1. **Product intent** — what the product should become (`pitches/`, `briefs/`)
2. **Committed scope** — frozen baseline when a bet starts (git tag on the pitch)
3. **Delivery state** — implemented + verified (change specs in the product's code repo)
4. **Production state** — actually running (script-derived, never hand-maintained)

Durable artifacts are **decisions and evidence** (`system/records.md`), not requirement docs. Specs derive from them and are expected to churn. Changes to committed scope are explicit deltas (PRs), never silent edits.

## The loop

```
signal → evidence log → PM interview → flow addendum → [debate | sanity pass] → pitch (frozen, flows tagged with it)
      → work packets → design gate → data gate → build → working-software inspection → acceptance → retro
```

Every station is mandatory or explicitly waived in writing. The **flow addendum** (`system/flow-addendum.md`) enumerates the human journeys at pitch time, in scope for debate and frozen with the pitch — the design gate renders them, never discovers them (SYS-DEC-004). The packet station (`system/work-packets.md`) derives user stories, edge states, and UX artifacts from those flows mechanically — the owner never prompts for "peripheral" work. The **design gate** (`system/design-gate.md`, layered design review) and **data gate** (`system/data-gate.md`, domain truth + seams) are owner sessions before any build; the data gate may loop back to the design gate when new surfaces emerge.

- **Signal capture**: human feeds raw signal (calls, dogfooding, inspiration) into the product's `evidence/`. Every item is provenance-tagged `ground-truth` (human-originated) or `inference` (agent-produced). A pitch resting only on inference is flagged.
- **PM interview**: interviewer agent drafts a discussion-guide agenda (`system/discussion-guide.md`), shows it for strike/add edits, runs a freeform interview with coverage-checklist semantics, outputs a structured brief to `briefs/`.
- **Adversarial review**: selective — trigger rules in `system/debate-triggers.md`. Skipping is a recorded decision, never an accident. The panel of lenses is **derived per bet** (`system/debate-heuristics.md`) and executed as isolated processes (`system/debate-runner.md`). Output is an argument graph (`system/claim-schema.md`); the memo is generated from the graph.
- **Pitch**: Shape Up shape — problem, bound, rough solution, rabbit holes, no-gos. Frozen as the baseline (git tag `pitch/<product>-<n>`). The bound is fixed budget, variable scope — denominated in owner review sessions and/or a real-world deadline, never time estimates (`system/decisions.md` SYS-DEC-001).
- **Build**: work packets carry stable IDs, pinned baseline, acceptance scenarios, explicit non-goals. Requirement deltas are PRs against the pitch, never silent rewrites.
- **Retro**: bet closes with a short note ("bound was X, took Y sessions, because…") attached to the pitch. PM judgment accretes one file per bet.

## Starting a new product

1. **Add it to `portfolio.yaml`** with a tier and status. The tier decides how much of the loop applies — pick it deliberately, because it is the only sanctioned way to cut corners.
2. **Create `products/<name>/`** with `evidence/`, `decisions/`, `briefs/`, `flows/`, `debates/`, `pitches/`.
3. **Start with evidence, not a plan.** Write down what the human has already said, verbatim, one item per ruling (`system/records.md`). An empty evidence log means there is no bet yet — go get signal. A first pitch written before any `ground-truth` evidence exists is the failure this ordering prevents.
4. **Run the loop in order.** Each station's spec is below. A station is either done or waived in writing; there is no third state.
5. **Record system-level lessons where you found them.** When a station fails in a way the spec didn't anticipate, fix the spec in `system/` and record the ruling in `system/decisions.md` — the product artifacts are not the place to encode process learning. Every `SYS-DEC` in this repo came from a station failing exactly once.

## The specs

| Station | Spec | Maturity |
| --- | --- | --- |
| Records (evidence + decisions) | `system/records.md` | specified |
| PM interview | `system/discussion-guide.md` | specified |
| Flow addendum | `system/flow-addendum.md` | specified |
| Debate — whether | `system/debate-triggers.md` | specified |
| Debate — panel design | `system/debate-heuristics.md` | specified |
| Debate — execution | `system/debate-runner.md` | specified |
| Debate — output format | `system/claim-schema.md` | specified |
| Work packets | `system/work-packets.md` | specified |
| Design gate | `system/design-gate.md` | specified |
| Data gate | `system/data-gate.md` | thin — covers what the brief must contain, not how the session runs |
| Working-software inspection | — | unspecified |
| Acceptance | — | unspecified |
| Retro | — | unspecified |

**The maturity column is load-bearing.** Every spec here was written or rewritten after the station it describes failed in a specific way; the ones marked thin or unspecified have not been run enough to have failed informatively yet. Treat them as intent, run them by judgment, and write the spec from what goes wrong. Do not treat an unspecified station as optional — it is unwritten, which is different.

## Tiers

Recorded per product in `portfolio.yaml`; ceremony scales with stakes deliberately.

| Tier | Process |
| --- | --- |
| `experiment` | Freeform chat → agent spec. No loop overhead. |
| `real` | Pitch + evidence log + work packets. Debate only when triggered. |
| `client-facing` | All of the above + decision log + deploy-state tracking. |

## Layout

```
portfolio.yaml                  # product index: tier, status, repo pointers
system/                         # the operating-system specs (product-agnostic)
products/<name>/
  evidence/EV-###-slug.md       # provenance-tagged signal
  decisions/DEC-###-slug.md     # chose A over B because C
  briefs/BRIEF-###-slug.md      # interview output
  flows/FLOWS-###-slug.md       # human journeys — pitch addendum, frozen with the pitch
  debates/DEBATE-###-slug.{yaml,md}  # claim graph + memo generated from it
  pitches/PITCH-###-slug.md     # frozen baselines (tagged at freeze)
```

Change specs (OpenSpec-style) live in each product's **code repo**, not here. Deploy state is stamped by script, not typed. The human-facing dashboard is deferred until the pitch discipline proves itself; this repo stays fully legible without it.
