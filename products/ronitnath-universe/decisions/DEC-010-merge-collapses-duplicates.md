---
id: DEC-010
date: 2026-08-05
source: owner ruling on DEBATE-002 escalation 5 (C-12)
---
Chose **merge produces one person, with no preserved trace of the two rows**, on the owner's reasoning: "post-merge counts as one person; theoretically the two people were duplicates anyways."

The two rows were never two people — they are an artifact of three codebases that each re-created the same humans (EV-013). So a merge is a *correction*, not a join, and there is nothing about the duplication worth keeping.

**Rules that follow, all cheap because responses are already an append-only log (DEC-008):**

| Fact | Merge behaviour |
| --- | --- |
| Response log | Both logs concatenate under the survivor. The current answer for any event is the **latest revision** — if the duplicates answered the same event differently, the later answer is the person's more recent intent. |
| Attendance | If **either** record says they showed, they showed. Unknown only when both are unknown (DEC-008 keeps unknown distinct from "did not attend"). |
| Capability links | **All links from both rows keep working**, now resolving to the survivor. A friend holding an old link must not be broken by an owner-side tidy-up. |
| Contact / nickname | Survivor keeps one contact record; the owner picks the nickname when the two differ. |
| Copy counts (PKT-14) | Summed — the owner did copy that many times. |

**Deletion is archive, not delete.** H8-failure-realist-5 found that the predecessor foreign keys **cascade-delete attendance**, so a naive delete destroys exactly the data this bet exists to preserve. No merge or archive path may cascade into the response log or attendance; those rows survive their identity's archival.

**What the archivist wanted and does not get**: provenance of the distinct imported rows (H2-archivist-6). With DEC-008 already giving up page reconstruction, retaining duplicate-row lineage has no remaining question it answers. Given up knowingly.

**What would change this**: a merge that turns out to have combined two *genuinely different* people. There is no undo. If that happens once, the answer is a confirmation step naming both rows' event histories before the merge commits — not a retained trace.
