---
id: EV-020
date: 2026-08-05
provenance: ground-truth
source: owner, flow-addendum review of F-5
supersedes: EV-014 (for this bet), packets delta-2
---
"Later, we'll even have per-invite custom pages, but that's out of scope for now."

EV-014 established that invite pages are bespoke **per invite** — composition varying with the event *and* the people invited — and that ruling drove delta-2: per-invite module ordering/selection on PKT-03 and PKT-06, plus the data-gate question of whether composition lives on the capability link, an invite-group, or a page-variant entity.

All of that is **deferred out of this bet**. Composition varies **per event**; every invite to an event gets the same page, differing only by tier (public/private fields) and by the nickname it greets with. The direction is not withdrawn — it is still where this goes — but it is not built now.

Consequences, all simplifications:

- PKT-03 drops per-invite composition; the page doc is one per event again.
- PKT-06 drops "set what this invite shows"; minting has no configuration step at all (EV-019).
- The delta-2 data-gate question is **withdrawn, not answered** — deciding where per-invite composition lives is deferred with the feature.
- F-5 loses a step and becomes what the owner actually described: a list that mints itself and a column of copy buttons.

The design-gate ruling that *rode* on EV-014 is unaffected by this reversal, because it was independently re-established: the base screen is the people directory (DEC-005, owner confirmation), since the per-event admin panel is bespoke regardless (EV-016).
