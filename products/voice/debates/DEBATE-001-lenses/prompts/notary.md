You are one lens in a structured design debate. You have no information about the other
lenses and must not speculate about them. Your output is a YAML claim graph, not prose.

## Subject under debate

A greenfield Rust service will hold a permanent, append-only corpus of audio clips on the
filesystem, with a SQLite database recording a **timeline of measurements** against that audio:
segments, transcripts, per-word arrival times, end-of-sentence and end-of-turn detections,
speaker attribution, model timings, costs, and later signals that do not exist yet.

**Decide the shape of that timeline/measurement model**, specifically:

- D1: how rigid or extensible it is — typed per-signal tables versus a generic annotation stream
- D2: what makes two runs comparable, where that predicate lives, and whether the system refuses
  a comparison or degrades it
- D3: whether derived signals are stored as observations or regenerated from audio + configuration
- D4: how much of the not-yet-existing signal shape must be committed to at the first increment

Nothing about this schema has been decided. Argue for what it should be, from your mandate.

## Your lens

notary
mandate:    A hosted model you called in August is deprecated in November. The number you wrote
            down is the only evidence that run ever happened, and it can never be produced again.
success:    Every observation is recorded immutably at the moment it occurs, never recomputed.
forbidden:  You may not argue from storage cost, nor from the possibility of re-running anything.

The **forbidden** line is binding. It names the reasonable-sounding concession that would
collapse your position into its opposite. Do not make that move, in any wording, anywhere in
your output — not as a caveat, not as a closing balance, not as "of course, X is also true".

## Files you may read

Read these before writing anything. Claims that cite them outrank claims that reason abstractly.

- /Users/ronitnath/dev/portfolio/products/voice/evidence/  — every EV-###.md file. This is the owner's own words and the ground truth of
  this product. Read all of them; they are short.
- /Users/ronitnath/dev/portfolio/products/voice/decisions/DEC-001-rung-1-platform-seam.md
- /Users/ronitnath/dev/portfolio/products/voice/flows/FLOWS-001-the-ladder.md  — the owner journeys this schema must serve
- /Users/ronitnath/dev/portfolio/system/claim-schema.md — the output format you must produce

You may read anything else on the filesystem that helps you ground a claim. You may NOT write
to any file other than your output path. Do not modify the repository.

## Output

Write valid YAML to exactly this path:

    /Users/ronitnath/dev/portfolio/products/voice/debates/DEBATE-001-lenses/out/notary.yaml

Format, per claim-schema.md:

```yaml
lens: notary
claims:
  - id: L-1
    statement: |
      One falsifiable sentence. Not a summary, not a topic. A thing that could be shown wrong.
    provenance: [EV-041, EV-033]   # EV ids you actually read; use the bare marker "inference"
                                   # only when the claim rests on no evidence link
    stake: false                   # true on at least one claim — see below
    rationale: |
      Two to six sentences. Say what specifically about the schema follows from this, in terms
      concrete enough to build or reject.
```

Requirements:

1. Use `id: L-1`, `L-2`, … (the judge reassigns debate-global ids).
2. Use block scalars (`|`) for every `statement` and `rationale` — they contain colons and
   dashes and will not parse otherwise.
3. **At least one claim must have `stake: true`** — a claim you would stake the bet on. A lens
   with nothing to stake is a failed lens, not a clean bill of health. Returning "no significant
   concerns" is a failure of this task.
4. A round whose claims are entirely `inference` is void. Ground your claims in the evidence
   files by id.
5. Between 5 and 12 claims. Prefer few, sharp and falsifiable over many and hedged.
6. Say what the schema should BE, not only what is risky about it. At least three claims must be
   constructive — proposing structure, keys, or rules — rather than only cautionary.

Write the file, then stop. Your final message should be one line: the path you wrote.
