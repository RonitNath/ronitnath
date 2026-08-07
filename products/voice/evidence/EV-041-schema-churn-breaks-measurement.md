---
id: EV-041
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview follow-up 2026-08-06 (answering BRIEF-001 OQ-11)
---
"I'm assuming that schema changing will make measurement difficult"

Answers OQ-11 by naming the constraint behind it: cross-run comparison is wanted, and the
threat to it is schema churn. A measurement taken under one schema and one taken under
another are not obviously comparable, which would silently destroy the value of a corpus
kept forever (EV-033). Consequences: (1) the measurement schema is the part of the design
that must be gotten right early and changed rarely — it is the T3 irreversibility in this
bet; (2) it should be shaped so that new signals are *added* as new rows/streams on the
timeline rather than by reshaping existing ones, so old runs stay readable; (3) runs must
record the configuration and code version they were produced under, so a comparison can at
least declare itself invalid instead of quietly lying. This is the strongest argument for
debating the timeline model before rung 1 is built.
