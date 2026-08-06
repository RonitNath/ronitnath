---
id: DEC-009
date: 2026-08-05
source: owner ruling on DEBATE-002 escalation 4 (C-11)
---
Chose **no guest-facing disclosure surface, and yes to recording link opens**, against the guest-dignity lens on both counts.

Owner: "no; yes."

**No disclosure.** A guest is not shown what is stored about them, is not given a correction or deletion route, and is not told the retention period. H4-guest-dignity-1 argued F-7 asks for status, party size, a note and segment answers without ever naming what is kept — accepted as accurate, declined. This is a private platform for the owner's friends, not a service with users; the relationship carrying the trust is the friendship, not a privacy notice.

**Link opens are recorded.** `uses` and `last_used_at` continue to be written on every capability GET. H4-guest-dignity-6 called this "an undisclosed behavioural event" — it now is a *decided* one. It answers the question the owner actually asks as an event approaches: who has and hasn't looked.

**The choice is recorded here so it isn't rediscovered as an accident.** The panel's objection was the only argument the guest's side has ever had in this bet, and it lost on the merits of a small private product. If ronitnath.com ever becomes something people join rather than something friends are invited to, this decision is the first one to revisit — and DEC-009 exists so that revisit starts from a stated position rather than an archaeology exercise.

**Not covered by this ruling — a correctness bug, fixed regardless (C-11 first half).** A personalized link is a bearer capability, so whoever opens a forwarded one can read *and silently overwrite* the intended invitee's answer, because F-7 binds every return through that link to one identity. The minimum fix is not disclosure and not authentication: the page states whose invitation it is and whose answer is being changed ("You're answering as Nikhil"), so a partner opening a forwarded link sees it isn't theirs. The bearer model is unchanged.

## Amendment, 2026-08-06 (EV-024) — the prescribed fix was wrong; the bug is closed anyway

The paragraph above prescribes a *conditional* rendering, and the owner's question at the design gate
— "how would the link know if it's been forwarded or not?" — has no answer. A bearer link cannot
detect forwarding; every request through it is indistinguishable from the intended recipient's. The
banner was a warning fired on a detection that does not exist.

**Replaced by an unconditional headline: `{person_name}'s invite to {event_title}`**, rendered
identically every time the page is served. Whoever opens it reads whose answer they are about to
change, and the system never claims to know how the link travelled — which is what the banner was
actually for. There is no forwarded *case*, because there is nothing to branch on.

Everything else in DEC-009 stands unchanged: no guest disclosure surface, no correction or deletion
route, and link opens continue to be recorded.
