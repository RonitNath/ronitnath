# Spec: PM interview — discussion-guide pattern

The interviewer is a separate role from the spec-writer. Its output is a structured brief (`briefs/BRIEF-###`), not a spec.

## Procedure

1. **Draft the agenda first.** Before the first question, write a topic agenda covering the whole interview end to end. Hold it at **pitch altitude** — the standard topics below, adapted to the bet. Cite the evidence items the agenda already leans on.
2. **Show the agenda upfront** for five-second strike/add edits. Do not begin questioning until the human has seen it (accepting silence/"go" as approval is fine).
3. **Interview freeform.** The agenda is a coverage checklist, not a script. Answers often cover other topics incidentally — mark them covered instead of re-asking. No hard question budget.
4. **Terminate on coverage, not count.** The interview ends when every agenda item is covered or explicitly struck. Every question must trace to an uncovered agenda item; a question that can't is drift — don't ask it.
5. **Sub-altitude curiosities are logged, not asked.** Record them as named open questions (`OQ-#`) in the brief. Rule: *record when uncertain, escalate when blocking.* Open questions are inherited by the debate (or sanity pass).

## Standard agenda topics (pitch altitude)

| Topic | What it must yield |
| --- | --- |
| Problem | Why this bet, why now; what hurts today (tie to evidence) |
| Users | Who touches this; whose behavior changes |
| Bound | The bet's fixed budget — owner review sessions and/or a real-world deadline, never time estimates (SYS-DEC-001) — and whether this is one bet or several |
| Constraints | Hard requirements: compatibility, migrations, rulings already made |
| No-gos | What is explicitly out, even if adjacent and tempting |
| Success signals | The observable event that marks the bet won |
| Risks / irreversibility | What is expensive to walk back (feeds debate triggers) |

## Brief format

```
BRIEF-###-slug.md
---
product, bet, date, interviewer, status: draft|final
confidence: high|medium|low        # interviewer's own flag; low → debate trigger
---
## Agenda coverage        # each topic: covered/struck + 1-line distillation
## Answers                # per topic, condensed, evidence-linked (EV-###)
## Open questions         # OQ-#: statement, why it matters, blocking? y/n
```
