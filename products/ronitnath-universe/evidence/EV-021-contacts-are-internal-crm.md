---
id: EV-021
date: 2026-08-05
provenance: ground-truth
source: owner, post-DEBATE-002
---
"The contacts surface never becomes something for others; that's purely my internal C(ontacts)RM. For the sake of this, the information I add about them that's customized (i.e. nickname) should be on a separate table, not part of the identities table."

Two rulings, one of them a schema constraint:

1. **Contacts is permanently owner-only.** It is not a stub for a future profile system. When friends eventually get accounts (later bet), whatever they maintain about themselves is a *different* thing living somewhere else — the contact record does not become their profile.
2. **Owner-authored data lives on its own table.** Nickname, and anything else the owner records *about* a person, sits on `contact`, never on `identity`. The identity row holds what the system knows a person to be; the contact row holds what the owner says about them.

This is the schema half of what H4-guest-dignity-5 argued in DEBATE-002 — that one universal identity row carrying an owner-authored nickname, invite targeting, guest self-description and attendance history would "silently turn the owner's guesses into the guest's identity." The disclosure half of that lens lost (DEC-009); **this half is adopted in full**, for the owner's own reason rather than the lens's: when friends can maintain their own records, the seam already exists and nothing has to be untangled.

Consequence: `contact` is not a nullable column-bag on identity, and code reading a person's *name* for a link slug (PKT-06) reads identity, while code rendering a greeting reads contact. A person can exist with no contact row at all.
