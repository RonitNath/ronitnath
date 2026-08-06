# Spec: evidence and decision records

The two durable artifact types. Everything else in this system — briefs, flows, pitches, packets — is
derived and expected to churn. These two accrete and are never rewritten, only superseded.

## IDs

`EV-###`, `DEC-###`, `BRIEF-###`, `FLOWS-###`, `DEBATE-###`, `PITCH-###`, `PKT-##`, `F-##`, `C-#`,
`OQ-#`, `SYS-DEC-###`. Numbers are allocated in order, are **stable for the life of the product, and
are never reused** — a superseded record keeps its number and says what replaced it. Filenames are
`<ID>-<slug>.md`; the slug may be corrected, the ID may not.

Everything downstream cites IDs rather than restating content. A packet naming a constraint cites the
decision; a claim cites the evidence. This is what makes it possible to ask "why is this like this"
and get an answer instead of an opinion.

## Evidence — `evidence/EV-###-slug.md`

```markdown
---
id: EV-###
date: YYYY-MM-DD
provenance: ground-truth | inference
source: who or what produced this
supersedes: EV-###          # optional
---
"<the human, verbatim>"

<what it settles, and the consequences that follow from it>
```

Rules:

1. **Quote the human verbatim.** The quote is the durable part; the surrounding reading is yours and
   may be wrong. Paraphrasing destroys the record's evidentiary value — an agent three months later
   cannot tell your inference from the owner's ruling, which is the whole distinction this system is
   built on.
2. **`provenance` is not a formality.** `ground-truth` means a human originated it. `inference` means
   an agent produced it. A pitch resting only on inference is a debate trigger (T6); a claim graph
   with no ground-truth links is void (`debate-heuristics.md` rule 2). Mislabelling one item quietly
   disarms both checks.
3. **One item per ruling.** When one message from the owner settles four things, write four items.
   Bundled items get cited for the one line someone remembers and the rest goes dark.
4. **Superseding is explicit.** A later ruling that reverses an earlier one names it in `supersedes`.
   The old item stays where it is — how the product changed its mind is itself evidence.
5. **Record it when it is said, not when it is needed.** Evidence written retroactively to justify a
   decision already made is the failure mode this whole layer exists to prevent.

## Decisions — `decisions/DEC-###-slug.md`

```markdown
---
id: DEC-###
date: YYYY-MM-DD
source: what forced it — an owner ruling, a debate escalation, a gate
---
Chose **X** over **Y**, because Z.

<the rules that follow — usually a table; each one a thing the build must now do>

**What was given up knowingly**: <the strongest thing the losing option had>

**What would change this**: <the observation that would reopen it>
```

Rules:

1. **"What would change this" is mandatory.** A decision without it is a preference. It is also the
   only cheap way to reopen the question later without re-running the argument — the record tells you
   what to look for.
2. **Record what was given up, by name.** The losing option's best argument is the thing you will
   rediscover under pressure and mistake for a new idea.
3. **Decisions carry their consequences.** A decision that only states a choice makes the build
   re-derive the rules. Write the table.
4. **A decision is not a spec.** It says what was chosen and why it binds. How to build it lives in
   packets, which cite it.

System-level decisions — rulings about this operating system rather than about a product — go in
`system/decisions.md` as `SYS-DEC-###`, same shape.

## Where records outrank everything else

When a spec, a pitch, or an agent's reasoning conflicts with an evidence item or a decision record,
the record wins and the other artifact is wrong. That is the point of keeping them separate from the
documents that churn.
