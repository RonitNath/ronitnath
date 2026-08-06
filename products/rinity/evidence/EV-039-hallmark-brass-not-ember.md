---
id: EV-039
date: 2026-08-06
provenance: ground-truth
source: owner, at PITCH-001 freeze (portfolio session 2026-08-06)
---
"rinity is currently using hallmark ember, but it should use brass instead."

rinity's console identity is **hallmark brass** (`@isoastra/tokens`; source of truth
`~/dev/isoastra/hallmark/web/styles/contract/style-brass.css`). The current build carries
ember — the red *internal* identity — which is wrong for an external product
(`unified-tokens.md`: ember = red-internal, brass = gold-external; EV-015 makes rinity
external). The switch is a design-gate constraint and a build change.
