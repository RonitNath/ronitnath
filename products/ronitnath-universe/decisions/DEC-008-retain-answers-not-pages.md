---
id: DEC-008
date: 2026-08-05
source: owner ruling on DEBATE-002 escalation 2 (C-6 ↔ C-7)
---
Chose **retain what people answered and whether they showed, not what they were shown**, resolving the minimalist/archivist collision in the minimalist's favour with one narrowing.

Owner: "I don't actually care to retain what the user saw, just what they answered (as audit log), and my internal data about whether they showed up."

**Upheld — DEBATE-001 C-6 survives.** Per-event visual identity, composition and field visibility stay **code** behind a release pointer. No page-document rows, no module-instance rows, no theme rows, no rendered-page snapshots. The archivist's C-7 is accepted as *true* and judged *not worth its cost*: a 2031 Postgres dump will not reconstruct what an invite page looked like, and that is a deliberate loss.

**Narrowed — two things must survive the code.**

1. **Responses are an append-only audit log.** A person's answer is never destructively updated; each submission is a new immutable revision (who, event, status, party size, note, segment answers, when). The current answer is the latest revision. This kills H2-archivist-5 (a stored combination nobody submitted) and satisfies H8-failure-realist-4 without any page history: one idempotent revision, one transaction.
2. **Attendance is a separate, owner-recorded fact.** Not derived from RSVP status — that derivation is what would make the people directory lie (H2-archivist-7). It is the owner's own observation, recorded when an event closes (new flow F-11).

So the three facts the directory shows are distinct and independently sourced: **invited** (a capability link exists), **answered** (the response log), **showed** (the owner's record).

**Falls away with this ruling:**

- Copy *history* is not required. C-3's concurrency defect still stands — an optimistic precondition on write must reject a stale save rather than clobber it — but nothing needs to retain superseded wording, because no downstream question depends on it.
- The archivist's retention pressure on revoked URLs and superseded identities loses its strongest justification, which narrows escalation 5 to a plainer question about merge and archive.

**What would change this**: if the owner ever wants to answer "what did the invitation actually say when she agreed to come", nothing in the schema can reconstruct it. That question is being given up knowingly.
