---
id: EV-033
date: 2026-08-06
provenance: ground-truth
source: owner, requirement 3 (portfolio session 2026-08-06)
---
"the agent should be intelligent about offering slots, only giving ~2 per week, then going to
the next week. This way, callers can't enumerate the slots, and also it puts pressure on
picky callers since the date they get gets further and further out. The office should be
able to customize this behavior"

Slot offering is a **strategy, not a dump**: ~2 offers per week, then advance to the next
week. Two deliberate effects: callers cannot enumerate the schedule (anti-probing), and
pickiness carries a cost (the offered date recedes). Per-office customizable — the offer
policy is config (F-7), not hardcoded behavior.
