# EV-043 — Design round-4 owner rulings (gate session ②, round 5)

- Type: ground-truth (owner statement, 2026-08-06)
- Source: design-gate session ②, corrections after round 4 + the remaining-pages enumeration
- Supersedes/amends: `system/design-gate.md` layer list; PKT-01, PKT-03, PKT-06, PKT-07

## Verbatim rulings

> "The component gallery shouldn't be part of the gate. Remove that from the default isntruction
> set."

> "For the scheduling desk, this is actually more complicated and invovled than you might expect.
> The pracice owners expects to see data in a number of different views, and to be able to interact
> with all of them. The regular is week view, but there's also agenda view, being able to see
> multiple days, and also a single-day view. Further, week view is split between workdays and
> including weekdays. On each of these, the user should be able to click and drag in order to add a
> new event with an arbitrary length of time to the calendar. Further, clicking on events should
> open a dialogue right next to it which allows editing, and click+drag should reschedule. Later,
> this will also plug into a system which proposes notifications to send via email, text, or phone
> call, but we don't need that feature yet. Further, some practices may have multiple providers in a
> single office, or multiple offices. We should consider how to handle that."

> "For onboarding, the website scraping procedure is also invovled. First, its a background task
> which needs to stream updates and is potentially interruptable and needs to play well in a HA
> application. Second, its findings may be contradictory, or contradict settings the user has
> already configured. It should be clear what that agent decided was obvious enough to
> do-first-then-ask and there should be a queue to approve specific items, which also has inline
> editing."

> "For editing, what I mean is that for any of these, you can just click on the text and start
> editing. There's no separate editing modal, it's all inline and automatically saved."

## Consequences

1. **Component gallery struck from the design gate** — removed from `system/design-gate.md`
   (layers renumbered), `work-packets.md`, `decisions.md`. Rationale on record: the base screen and
   real flow boards already show every component in context. Applies to all products, not just
   rinity; rinity COVERAGE §5 drops the L2 layer.
2. **Schedule desk is a full calendar product surface** (PKT-06):
   - Views: week (the regular), split **workdays vs full week**; **agenda**; **multi-day**;
     **single-day**. All interactive, not read-only renderings.
   - **Drag-create**: click+drag on empty grid creates an event of arbitrary length in every view.
   - **Click-to-edit**: clicking an event opens an editing dialogue **right next to it** (popover,
     not a modal or a separate page).
   - **Drag-reschedule**: click+drag an existing event moves it.
   - **Deferred, named**: a later system proposes notifications (email / text / phone call) about
     schedule changes — explicitly not in this bet, but the surface should not design it out.
   - **Multi-provider / multi-office**: some practices have multiple providers in one office, or
     multiple offices. Owner asked for a considered position (not yet a ruling).
3. **Website scrape is a first-class background task** (PKT-07):
   - Streams progress updates to the console (SSE per EV-041), is **interruptible/resumable**, and
     must behave correctly in an HA deployment (another replica can pick it up; no lost or
     duplicated findings).
   - Findings can be **contradictory** — with each other or with settings the user already saved.
   - The console must show the split between what the agent judged obvious enough to
     **do-first-then-ask** (applied, visibly reversible) and an **approval queue** of specific
     items awaiting the user, each editable inline in the queue itself.
4. **Inline editing is the design language's editing model** (PKT-01 constraint, retroactive to all
   surfaces): click the text and start typing. No separate editing modal, no explicit Save —
   changes are **saved automatically**. This amends the round-3 knowledge-base edit state
   (EV-042 mock had Save/Cancel buttons): the accent-bordered in-place editor stays, the
   Save/Cancel buttons go; the affordance for persistence is a quiet "saved" indication.
