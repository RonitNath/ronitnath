# Spec: adversarial-review trigger rules

Not everything gets a debate. Skipping must be a decision, not an accident: every pitch records one debate line either way.

## Triggers — any ONE fires a debate

| # | Trigger | Default threshold (revisable by decision, not by drift) |
| --- | --- | --- |
| T1 | Product tier | `client-facing` → always debate |
| T2 | Appetite | > 2 weeks of build time |
| T3 | Irreversibility | Schema/data-model commitments on stored user data; public URL or API commitments; identity/auth model changes; key-custody choices; anything whose reversal costs more than the appetite |
| T4 | Evidence conflict | Two evidence items contradict on a load-bearing claim of the brief |
| T5 | Interviewer confidence | Brief flagged `confidence: low` |
| T6 | Provenance | The brief's supporting evidence is entirely `inference` (no ground truth) |

Tier `experiment` never debates (T1–T6 don't apply); that is what the tier means.

## Below threshold: sanity pass

One agent, one page, **fatal flaws only** — wrong-problem, impossible-constraint, already-exists. No elaboration, no improvement suggestions.

## Record line (mandatory, in the pitch)

```
debate: skipped (tier=real, appetite=3d, no triggers)
debate: sanity-pass (tier=real, appetite=1w) → no fatal flaws
debate: triggered (T3: identity data model) → memo DEBATE-###
```

## When a debate runs

- Inherits the brief's open questions. Each ends resolved, deferred-as-assumption ("assume X; cheap to reverse"), or escalated.
- Escalation to the human only for questions **both blocking and undecidable by agents**, batched as one consolidated round — never a drip.
- Output is an argument graph per `claim-schema.md`; the memo is generated from it. Artifacts are decision-shaped: options considered, rejected-and-why, what would change the recommendation.
