---
id: EV-029
date: 2026-08-06
provenance: ground-truth
source: owner, data gate (session ③), answering a question about session records
---
"Further, I really only care about whether the person has opened the link. Raw opening is a weak
signal because if I send it over imsg then the preview system opens it, and over instagram meta
crawls it like 11 times."

**A GET is not a person, and every surface that promised "who has looked" was built on one.**

DEC-009 ruled that link opens are recorded, on the reasoning that it answers the question the owner
actually asks as an event approaches: who has and hasn't looked. That reasoning is intact; the
*measurement* was not. The owner shares links by iMessage and Instagram, and both fetch the URL
server-side to build a preview — so the invitee list's used / last-used / never-used column, and the
directory's *invited* fact behind it, would have shown a link as opened before it reached a human,
and shown eleven opens for one share.

**Two facts, not one** (DEC-014): a raw `fetch_count` (kept — it is free, and it is the anti-bot input
named in EV-030) and a confirmed `open_count`, which is what every surface shows.

Worth recording as its own item because of *how* it arrived: the question on the table was whether
session records should store a coarse location. The owner answered that, then volunteered this — a
defect in a different table, found because he was thinking about what the product is actually able to
observe. The same instinct produced EV-024 at the design gate ("how would the link know if it's been
forwarded?"). Both are the same check: **does the system actually see the thing this number claims
to count?**
