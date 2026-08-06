# Spec: running a debate against a real harness

`debate-heuristics.md` says what a panel is. This says how to actually run one, and why the obvious
way doesn't work.

## Why lenses are separate processes

The tempting implementation is: the agent holding the pitch spawns sub-agents, one per lens, and
aggregates their answers. It fails on three counts, and all three are the point of the exercise.

1. **Shared context recorrelates them.** The thing being decorrelated *is* the context. Sub-agents
   that inherit the parent's conversation inherit its framing, its half-formed conclusions, and its
   attachment to them. They will disagree about details and agree about everything load-bearing.
2. **The parent becomes a summarising channel.** Anything the parent writes to brief a lens is the
   author's account of the artifact under attack. Rule 1 of the round exists to prevent exactly this.
3. **The author is not a neutral judge** (rule 6). Making the author also the orchestrator makes the
   compromise total instead of partial.

So: **one OS process per lens, each with its own context, launched with file paths rather than
prose.** The orchestrating agent's job is to write the panel, launch, and fan in — not to explain the
artifact to anyone.

## Procedure

1. **Write the panel first.** Axes, then lens cards (mandate / success / forbidden), then the file
   list each lens may read. Nothing launches until the asymmetry is decided on paper; deciding it
   while launching is how every lens ends up with the pitch.
2. **One prompt file and one output path per lens.** The prompt carries: the lens card verbatim, the
   file paths it may read, the claim schema (`claim-schema.md`), and the instruction to write YAML to
   its own output path. Nothing else — no context about the round, no other lenses' names.
3. **Launch all lenses concurrently, detached.** Serial runs leak: a later lens can observe earlier
   output on disk. Concurrency is what keeps them blind to each other. Give each its own log file.
4. **Poll for completion by counting the runner processes**, matched on the launcher's own name.
   Matching on the model string does not work — model identifiers appear in the argv of unrelated
   processes and are absent from the argv of some of the ones you care about. Count what you started.
5. **Validate each output parses before fan-in.** Claim statements contain colons and em dashes;
   require a block scalar for `statement:` and quote any bare scalar containing `:`. A lens whose file
   doesn't parse has not returned a null result — it has returned nothing, and must be re-run.
6. **Fan in.** Merge claims into one graph, assign debate-global IDs, then read every lens's claims
   together looking for contradictions. Each contradiction becomes a `rebuts` edge with an explicit
   resolution. This step cannot be split across contexts — finding collisions requires holding all the
   claims at once, which is what caps panel size.
7. **Record the round's own provenance on the debate header**: which lens held which context, which
   model and family each ran, who judged, and whether the judge authored the artifact.

A concrete launcher — the shape, not a requirement:

```bash
# one process per lens; different model family from the orchestrator where stakes justify it
for LENS in "${LENSES[@]}"; do
  <agent-cli> --model "<decorrelated-model>" "$(cat prompts/$LENS.md)" > out/$LENS.log 2>&1 &
done
# wait on the launcher name, never on the model string
while [ "$(ps -eo command | grep -c '[l]ens-runner')" -gt 0 ]; do sleep 30; done
```

## Failure modes seen in practice

| Symptom | Reading | Response |
| --- | --- | --- |
| A lens returns only `inference` claims | It never opened the repo | Void that lens (rule 2), re-run with explicit paths |
| A lens returns "no significant concerns" | Failed lens, not a clean artifact | Re-run; if it repeats, the axis was fake — the decision was already made |
| Every lens agrees | The panel was inherited, not derived | Go back to axis derivation; agreement on an untouched axis is not evidence |
| Output won't parse | Colons in claim text | Block scalars; re-run the lens, don't hand-repair its claims |
| Claims cluster on one artifact | Asymmetry wasn't enforced | Check which lenses were handed the pitch |

## What this does not fix

The orchestrator still writes the panel, and the panel decides what can be found. A blind spot shared
by the author and the axis derivation survives the whole round. The owner reading the fan-in is the
only check on that, which is the honest reason the memo goes to a human rather than being actioned.
