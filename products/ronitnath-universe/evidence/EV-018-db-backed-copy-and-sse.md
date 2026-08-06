---
id: EV-018
date: 2026-08-05
provenance: ground-truth
source: owner, flow-addendum review
supersedes: EV-015's TOML-file mechanism (not its intent)
---
"I don't want to edit a toml file and trigger production. Instead, let's have it be db backed. There should be a page on admin for events where I can update the copy and it'll update the db. Further, I want SSE to all active pages and to push the copy updates immediately."

And, generalized: "prefer streaming updates via sse. As people respond to the invites, I want my admin dashboard to live update. Same with if people sign up."

Two rulings:

1. **Copy is database rows, edited in the console.** EV-015's *intent* survives — the owner edits wording directly, in one place, without touching structure — but the mechanism inverts: no file, no redeploy, no deploy pipeline in the middle of a typo fix. F-3 becomes a web flow with its own canonical console page, and PKT-11's unresolved delivery seam is resolved as option (c).
2. **SSE is the default update transport**, not an enhancement layered on later. Named surfaces: copy edits push to every active page for that event (guest pages included), RSVPs push to the admin dashboard, and identity changes push to the people directory.

Explicitly future, and the reason the seam is being built now rather than retrofitted: "Later, when people have richer surfaces for updating, I'd also want their updates to flow back to me in real time, but that's a feature for when we get to the point of allowing my friends/guests to login, rather than them just using a tokenized link." Friend accounts stay a pitch no-go; the transport they will need does not.

Also settled: work with the AI coding agent is recorded on flows as `surface: terminal`.
