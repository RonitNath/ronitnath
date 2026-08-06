# Spec: claim schema — debate argument graph

Debating agents emit **structured claims, not prose transcripts**. The written adversarial memo is *generated from* this graph so map and text cannot drift. Graph lives beside the memo: `DEBATE-###-slug.yaml` + generated `DEBATE-###-slug.md`.

## Claim

```yaml
claims:
  - id: C-1                     # stable within the debate
    statement: >                # one falsifiable sentence, not a summary
      Postgres row-level policy cannot express the circles level model without a view layer.
    author: <agent role>        # proposer | red-team | synthesis
    provenance:                 # evidence grounding; empty list is legal but visible
      - EV-004                  # ground-truth item
      - inference               # bare marker when the claim is agent reasoning with no evidence link
    supports: [C-3]             # edges to other claim ids
    rebuts: [C-2]
    resolution: resolved | deferred-as-assumption | unresolved | escalated
    note: >                     # required for any non-'resolved' status:
      Assumed cheap to reverse because …  /  Escalated: blocking + undecidable because …
```

## Rules

- Every open question inherited from the brief must appear as at least one claim, and its final `resolution` is written back to the brief.
- A claim with only `inference` provenance renders amber in any view; a consensus subgraph with zero ground-truth links is the primary failure smell.
- `escalated` claims are batched into one consolidated round-two for the human; no per-claim pings.
- The generated memo sections map 1:1 to the graph: **options considered** (claim clusters), **rejected and why** (rebutted-and-resolved), **recommendation** (surviving cluster), **what would change it** (the deferred assumptions).
