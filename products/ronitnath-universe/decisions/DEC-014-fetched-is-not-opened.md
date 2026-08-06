---
id: DEC-014
date: 2026-08-06
source: owner ruling at the data gate (EV-029)
amends: DEC-009 (which ruled that opens are recorded — that stands; what an "open" is does not)
---
Chose **two counters on a capability link — raw fetches and confirmed opens — with every owner-facing
surface showing only the confirmed number**, because the single counter DEC-009 authorized counts
link-preview crawlers as people.

**The defect.** The owner shares invite links by iMessage and Instagram. Both fetch the URL
server-side to build a preview: *"if I send it over imsg then the preview system opens it, and over
instagram meta crawls it like 11 times."* So `uses`, incremented on every capability GET, would mark
a link as opened before it reached a human and show eleven opens for one share. The invitee list's
used / last-used / never-used column and the "who has looked" question behind it were both built on
that number.

DEC-009's ruling is undisturbed — link opens *are* recorded, with no guest-facing disclosure. What
changes is what the product is willing to call an open.

| Fact | Written when | Shown where |
| --- | --- | --- |
| **`fetch_count` / `last_fetched_at`** | Every capability GET, unfiltered | **Nowhere owner-facing.** Diagnostic, and the anti-bot input named in EV-030. |
| **`open_count` / `first_opened_at` / `last_opened_at`** | The page confirms itself from the client | Everywhere "has looked" is answered: F-5's invitee list, F-9's directory |

**The confirmation costs nothing, which is why this is affordable.** The same session's OQ-3 answer
gives guest pages an SSE stream, and **a link-preview fetcher does not execute JavaScript, so it never
opens one**. The stream connecting on a tokened page *is* the open. No beacon endpoint, no tracking
pixel, and — importantly — no user-agent heuristic list, which would have to be maintained forever
against crawlers that change their strings.

**Consequences:**

1. The no-JS fallback (PKT-07) degrades to fetched-only. Accepted: an RSVP through the plain form is
   a definitive open and writes one anyway.
2. A sophisticated crawler that runs JS would register as an open. Neither named source does.
3. The confirmed write happens after first paint by construction, which also removes it from the
   render path (`data-model.md` §4.4).
4. **PKT-02, PKT-12 and PKT-14 are corrected, not extended** — a packet delta, since they currently
   specify the wrong fact.

**The pattern this belongs to.** PKT-14's counter was already narrowed once, by DEBATE-002 C-4, to
mean *copied* rather than *sent*, because clipboard success and actually sending fail independently.
This is the same correction one table over, and the rule is now general enough to state: **every
counter in this product names the event it actually witnessed.** Anything else is a label the data
cannot support.

**What would change this**: if link previews stop fetching, or if the owner starts sharing somewhere
that doesn't — then `fetch_count` becomes the honest number again and the second counter is dead
weight. Unlikely, and cheap to carry either way.
