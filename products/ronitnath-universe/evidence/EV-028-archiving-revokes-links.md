---
id: EV-028
date: 2026-08-06
provenance: ground-truth
source: owner, data gate (session ③)
---
"It should revoke."

Answering the data gate's third escalation. The brief had recommended that archiving a person leave
their capability links alive, on the reasoning that archival and access are two facts and that
DEC-010's no-cascade rule argues against propagation of any kind.

**The owner reversed it, and the reversal is better.** Archiving someone is the owner saying that
person is out of his directory; leaving them holding a working URL contradicts that in the one way
that is externally visible — they can still open the page and still overwrite an answer.

What survives of the original objection is *mechanism*, not outcome. The revocation is an explicit
update inside the archive transaction, **never a foreign-key cascade**. A cascade is a rule the
database applies to everything shaped alike, which is how the predecessors' FKs came to destroy
attendance data; this is one intended effect on one table, written down. Responses and attendance are
untouched, as they are by everything (DEC-010).

Consequence not asked about and taken as the plain reading: un-archiving does not un-revoke. The
links are gone and re-minting is how someone comes back.
