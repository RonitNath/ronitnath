---
id: EV-024
date: 2026-08-06
provenance: ground-truth
source: owner, second design-gate review
supersedes: DEC-009's "You're answering as Nikhil" fix (not DEC-009's rulings)
---
"I don't understand the answering-as, how would the link know if it's been forwarded or not?"

And, giving the replacement himself:

"Also, I'd want it to be, '{person_name}'s invite to {event_title}'"

**The question has no answer, and that is the finding.** DEBATE-002 C-11 correctly identified a real
correctness bug — a personalized link is a bearer capability, so whoever opens a forwarded one can
read and silently overwrite the intended invitee's answer. DEC-009's prescribed minimum fix was a
banner reading "You're answering as Nikhil". That banner is a *warning*, and a warning implies a
detection: it reads as though the system noticed something. **A bearer link cannot detect that it was
forwarded** — every request through it is indistinguishable from the intended recipient's, which is
what "bearer" means. The design gate drew the banner and the owner read the implied claim off the
screen immediately.

**The fix is unconditional, not conditional.** The invite page's headline is
`{person_name}'s invite to {event_title}`, rendered the same way every time it is served. Whoever
opens the link reads whose answer they are about to change, and the system never claims to know how
the link travelled. It achieves exactly what the banner was meant to achieve, with no detection in
the middle of it.

The bug DEC-009 named is closed by this; the bearer model is unchanged and no authentication is added.
The forwarded case is not a distinct rendering, because there is no distinct case — there is one
page, and it is honest to every reader of it.

**Generalisable, and the reason this is evidence rather than a design note**: a screen that fires a
warning on a condition the system cannot observe is a lie the data model will eventually be asked to
support. This one was caught at a design gate by the owner asking how the check would work. That is
the check.
