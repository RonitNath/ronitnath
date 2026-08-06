# Spec: the heuristic panel — how a debate gets real disagreement

A debate between agents fails by default, because agents spawned from the same model with the same
context produce the same opinion in different voices. Telling them to "be adversarial" doesn't fix
it — they perform disagreement and converge anyway. The fix is **structural**: give each agent a
different *job with a different success criterion*, so disagreement is a consequence of the mandate
rather than a request.

**There is no standing panel.** The lenses are derived per bet, from the decisions that bet actually
turns on. A panel carried over from another product produces polite agreement on the axes it doesn't
touch, and polite agreement reads as corroboration — which is worse than not running the round. This
spec is the method for building a panel, not a panel.

## Lens format

Each lens is three lines. The third is the one that does the work.

```
<name>
mandate:    the job, stated as a situation or a standard, not a topic
success:    what this lens is trying to be true when it is done
forbidden:  the move that would let it agree with its opposite
```

The **forbidden move** is what stops convergence. A lens told only what to care about will still
concede the other side's point in its final paragraph, because that reads as balanced. A lens
forbidden from arguing on cost grounds cannot make the concession that would collapse its axis. Write
the forbidden line by asking: *what is the reasonable-sounding sentence that would make this lens
agree with its opposite?* Then ban that sentence.

A mandate stated as a situation beats one stated as a value. "Judge this by X" produces an essay;
"it is 9pm, you are holding a phone, three people are talking to you" produces claims about specific
screens. Put the lens somewhere, not on a side.

## Deriving the axes

Lenses come in **opposed pairs**. Each pair is an axis the product genuinely sits on, and neither end
is correct — the value is the tension, and where two lenses collide is where a real decision is
hiding.

To find the axes for a bet:

1. **List the decisions the round has to serve.** The trigger that fired (`debate-triggers.md`) names
   at least one; the brief's open questions name the rest. Everything else is out of scope for the
   panel.
2. **For each decision, ask what would make it go the other way.** The answer names a value someone
   could hold. That value is one end of an axis.
3. **Name its genuine opposite** — the value that trades against it *in this product*. If you cannot
   state a case for the opposite that the owner might actually rule for, it is not an axis, and you
   have found a decision that is already made. Record it as a decision instead of debating it.
4. **Drop axes the decisions don't touch.** An axis with nothing at stake produces two lenses that
   agree, which looks like corroboration and is not.

Places axes reliably hide:

| Source | The axis it names |
| --- | --- |
| Two evidence items that contradict (T4) | The contradiction *is* the axis; the lenses are the two readings |
| The pitch's no-gos | Excluding something names the value that wanted it. Give that value a lens |
| Irreversible commitments (T3) | Cost-now against cost-to-reverse |
| Actors with divergent interests | One lens per actor, each forbidden from caring about the other |
| A capability the product is *for* | It against whatever holds the product together as one thing |

## How many lenses, and how many agents

**Two lenses per axis, one agent per lens.** The agent count is the axis count doubled — it is a
consequence of the decision surface, not a budget chosen up front.

- **Floor: one axis.** A single pair is a real debate. Below that there is no collision to produce, so
  the honest thing is a sanity pass (`debate-triggers.md`), not a one-agent "debate".
- **Ceiling: what one judge can hold at once.** Fan-in is the binding constraint — every claim from
  every lens has to be read together for collisions to be found. When the panel outgrows that, split
  into two rounds by decision cluster rather than skimming one large one.
- **Never an odd number to break a tie.** This is not a vote. Collisions are the output; a majority
  would hide exactly the disagreement the round exists to surface.
- Scale with the number of independent decisions, not with the size of the product. A large product
  making one reversible choice gets one axis.

## Rules of the round

1. **Asymmetric context, recorded.** A lens whose job is to attack a conclusion does **not** receive
   the document stating it — give it the evidence log and the code, not the pitch that argues for the
   answer. Which context a lens held is part of its claims' provenance. Pass file paths, never a
   summary: a summary written by the artifact's author leaks the author's framing into every lens.
2. **Grounding beats reasoning.** Lenses get repo and filesystem access. A claim citing an artifact —
   a schema, an old codebase, a real seed file — outranks one that reasons in the abstract. **A round
   whose claims are entirely `inference` is void** (extends the T6 trigger inward).
3. **The null result is expensive.** Every lens returns at least one claim it would stake the bet on.
   The judge records, by name, why each rejected claim was rejected. "No fatal flaws" must cost
   something to write. A lens with nothing to stake is a failed lens, not a clean bill of health.
4. **Collisions are the output.** Where two lenses contradict, the pair is recorded as a `rebuts` edge
   and resolved explicitly — resolved / deferred-as-assumption / unresolved / escalated
   (`claim-schema.md`). An axis with no collision means one of its lenses didn't do its job.
5. **Different model family where the stakes justify it.** The largest decorrelation available, and it
   costs a parameter. Recorded on the debate.
6. **The judge is compromised.** Whoever wrote the artifact under debate cannot be the sole
   adjudicator of it; their aggregation is itself an input the owner reviews. This is a known,
   unfixed limitation — stated so it isn't mistaken for rigour.

Execution — process isolation, launching, fan-in, and the failure modes: `debate-runner.md`.
