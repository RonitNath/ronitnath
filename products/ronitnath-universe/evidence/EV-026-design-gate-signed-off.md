---
id: EV-026
date: 2026-08-06
provenance: ground-truth
source: owner, design-gate sign-off (session ② closes)
---
"I'm approving this. The main thing I want to note is that this layout/design is still specific to
this event, and other events will likely have other designs. Later, we ought to build a library of
templates for events, and even this even needs more focused design work, because as-is it still looks
bad. However, I want to proceed with the next phase"

**The design gate closes.** Session ② of PITCH-001's five-session bound is spent. The tokens, the
component library and the canonical surfaces are the contract the build styles against.

**What was approved is the design *language*, not the event's look.** The owner's caveat is explicit
that the pickleball rendering "still looks bad" and needs focused work. That is consistent with
DEC-005 rather than a contradiction of it: the invite page and the event's admin panel are per-event
compositions, so what the gate locked is the vocabulary they draw from, and any single event's
composition is a separate piece of work. **This does not reopen the gate** — the owner ruled to
proceed — and it does not become a build task inside PITCH-001 either. It lands at session ⑤, the
pickleball-test acceptance, where the event that has to look good is the one being judged.

**A template library for events is named as later work**, not scoped here. It is the natural graduate
of DEC-005's revisit condition ("if per-event panels turn out to be the same panel with different
tiles…") and of DEC-007's ("if two events' extension tables turn out to be the same table…"). Both
say: revisit after two real events. A template library is what that revisit produces if the answer
comes back yes. Writing it now would be designing against one event, which is the failure the
predecessors already demonstrated three times (EV-013).

Consequence for the data model: **nothing in the schema may assume the pickleball composition.** The
gate's boards are one instance, and a second event with a different design must need no migration.
