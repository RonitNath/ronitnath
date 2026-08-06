---
id: DEC-011
date: 2026-08-05
source: owner rulings on EV-022 follow-up
---
Chose a **role-resolved root with a small explicit route surface**, no `/app` shadow space.

The distinction that makes this coherent, confirmed by the owner: **role decides what you may see; route decides which thing you are looking at.** Those are not in tension. Applying "no route-based content" all the way down would leave sub-surfaces with no URL, breaking refresh, back, and sending yourself a link to an event's panel.

| Route | Resolution |
| --- | --- |
| `/` | VISITOR → presence · OWNER → the personal dashboard (DASH-001) · FRIEND → their card set (later) |
| `/landing` | **Always presence, for every role.** The escape hatch: once signed in, `/` is your dashboard, so this is how you (and later your friends) look at the front of the site. |
| `/auth` | Sign-in now, **register later** — both live behind this one path rather than separate routes. |
| `/contacts`, `/contacts/{id}` | The CRM (F-9, F-10) |
| `/accounts` | Account management — one row today, the seam friend accounts arrive on |
| `/e/{event}` | VISITOR → public event page at public tier · **OWNER → that event's admin panel** · `?as=guest` renders the guest view for preview (F-2 step 5) |
| `/e/{event}/{person}-{hash}` | The capability link — name-slug plus short hash (EV-019) |
| `/e/{event}/copy` | Copy editing (F-3) |
| `/e/{event}/invites` | Invitee list (F-5) |
| `/stream` | SSE, role-scoped subscriptions (PKT-13) |
| `/api/…` | Agent API (PKT-04) |
| `/healthz` | Probe (PKT-09) |

**Events are namespaced under `/e/`**, not at the root. Root-level slugs are prettier, but every future platform word — photos, calendar, circles — wants a root slug, and events are created casually by an agent, so eventually `/photos` collides with an event someone named "photos". Reserved-word lists rot; a namespace doesn't.

**`/` renders, it does not redirect.** One request, one response, content chosen by role. A redirect would cost a round trip and put the user on a URL that isn't the one they typed. `/landing` exists precisely so that rendering-not-redirecting still leaves presence reachable.

**Operational consequence, and the main risk of this whole shape**: one URL serving different bodies per role must be `Cache-Control: private` with `Vary: Cookie`, and Cloudflare sits in front of this. A cached owner dashboard served to a visitor is silent and serious. Recommendation: do not cache `/` at all initially, and make it an acceptance test — fetch `/` with and without a session cookie, assert different bodies and correct headers.

**Why this beats an `/app` split on security, not just aesthetics**: an SPA-style `/app` ships the owner's UI to every visitor and guards it client-side. Role-resolved SSR never sends the owner's markup to someone who isn't the owner.
