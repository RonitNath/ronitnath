---
id: EV-023
date: 2026-08-06
provenance: ground-truth
source: owner, second design-gate review
supersedes: EV-019's nickname ruling (not its link-format or copy-tracking rulings)
---
"The nickname feature should be removed from this version; it was more a plugin, and I'd only want it situationally depending on the event."

**The nickname leaves the bet.** EV-019 had made it a first-class column: contacts carry an
owner-authored nickname, invite pages greet by it, and the directory shows it. The owner's reading is
that it is a *per-event* affordance — something one event wants and the next does not — which by
DEC-007 makes it extension territory, not a platform column.

What this removes: the nickname field on `contact`, the "greeted as" column on the invitee list
(F-5), the nickname on the directory (F-9), the nickname pick in the merge rule (DEC-010), and the
nickname fallback state in PKT-06. Pages greet by the identity's name.

What it does **not** remove: `contact` itself. EV-021's separation stands on its own reasoning — the
identity row holds what the system knows a person to be, the contact row holds what the owner records
about them — and `contact` keeps phone, email and anything else the owner authors. The seam survives
the loss of its first field.

Consequence worth naming: the nickname was the field that made EV-021's identity/contact split
*visible* in the design gate's panels. The split is now correct but undemonstrated, so nothing on a
screen will catch it if the build collapses the two tables. That check moves to the data gate and to
a test, where it should have been anyway.
