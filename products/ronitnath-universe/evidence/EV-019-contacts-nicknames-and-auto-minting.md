---
id: EV-019
date: 2026-08-05
provenance: ground-truth
source: owner, flow-addendum review of F-5 steps 2–3
---
On how invites actually get made and sent:

- **Identities link to contacts, where the owner puts a nickname.** A new entity beside identity; the nickname is owner-authored, not self-chosen.
- **Link URLs use the person's name plus a short hash**, the hash being what makes a link revocable — re-mint changes it and the old URL dies. **The event page greets by nickname.** Formal name in the URL, familiar name on the page.
- **No personalization at sharing time.** "Everything is minted automatically based on an invitee list (dynamic), and I'm mechanically doing sharing." There is no per-link configuration step: add someone to the list, their link exists. Adding a person later mints immediately — the list is live, not batched.
- **Copy-link buttons must remember.** "One helpful feature would be knowing which copy-link buttons I'd clicked, and how many times (changes color per copy-to-clipboard). When going down a long list of invites, this helps me visually keep track of who to invite next, and who I haven't invited yet (if I'm going out of order)."

The last one is small to describe and load-bearing in use: sharing happens outside the product, so the *only* trace the system can hold of "did I send this one" is the copy click. Without it a long invitee list is unreadable at the size that matters, and out-of-order work is impossible to resume. Consequence for the model: copy count is state on the link, not ephemeral browser state — otherwise it is lost on reload and split across the owner's laptop and phone.
