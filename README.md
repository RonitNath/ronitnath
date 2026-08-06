# portfolio — product operating system

Canonical product graph for all products. Solo operator, agent workers. Four kinds of truth, kept separate, joined by stable IDs:

1. **Product intent** — what the product should become (`pitches/`, `briefs/`)
2. **Committed scope** — frozen baseline when a bet starts (git tag on the pitch)
3. **Delivery state** — implemented + verified (change specs in the product's code repo)
4. **Production state** — actually running (script-derived, never hand-maintained)

Durable artifacts are **decisions and evidence**, not requirement docs. Specs derive from them and are expected to churn. Changes to committed scope are explicit deltas (PRs), never silent edits.

## The loop

```
signal → evidence log → PM interview → [debate | sanity pass] → pitch (frozen) → work packets → build → retro
```

Every station is mandatory or explicitly waived in writing; the packet station (`system/work-packets.md`) is where user stories, edge states, and UX artifacts are derived mechanically — the owner never prompts for "peripheral" work.

- **Signal capture**: human feeds raw signal (calls, dogfooding, inspiration) into the product's `evidence/`. Every item is provenance-tagged `ground-truth` (human-originated) or `inference` (agent-produced). A pitch resting only on inference is flagged.
- **PM interview**: interviewer agent drafts a discussion-guide agenda (`system/discussion-guide.md`), shows it for strike/add edits, runs a freeform interview with coverage-checklist semantics, outputs a structured brief to `briefs/`.
- **Adversarial review**: selective — trigger rules in `system/debate-triggers.md`. Skipping is a recorded decision, never an accident. Debate output is an argument graph (`system/claim-schema.md`); the memo is generated from the graph.
- **Pitch**: Shape Up shape — problem, bound, rough solution, rabbit holes, no-gos. Frozen as the baseline (git tag `pitch/<product>-<n>`). The bound is fixed budget, variable scope — denominated in owner review sessions and/or a real-world deadline, never time estimates (`system/decisions.md` SYS-DEC-001).
- **Build**: work packets carry stable IDs, pinned baseline, acceptance scenarios, explicit non-goals. Requirement deltas are PRs against the pitch, never silent rewrites.
- **Retro**: bet closes with a short note ("appetite was X, took Y, because…") attached to the pitch. PM judgment accretes one file per bet.

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
  debates/DEBATE-###-slug.{yaml,md}  # claim graph + memo generated from it
  pitches/PITCH-###-slug.md     # frozen baselines (tagged at freeze)
```

Change specs (OpenSpec-style) live in each product's **code repo**, not here. Deploy state is stamped by script, not typed. The human-facing dashboard is deferred until the pitch discipline proves itself on the pilot; this repo stays fully legible without it.
