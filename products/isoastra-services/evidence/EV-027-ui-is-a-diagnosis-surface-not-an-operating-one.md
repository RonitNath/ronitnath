---
id: EV-027
date: 2026-08-06
provenance: ground-truth
source: owner, PM interview (voice follow-up)
---
"in terms of the UI, I will almost never use the UI. If I do, it's because something's gone seriously wrong, and I want to see what configurations we have, or I'm bored, and I'm curious about what the routing configuration looks like. Same with emails, it's more of a 'wtf is going on right now' or if I want to look at the emailing volumes. Otherwise, it's mainly agents and services using this, and agents using this for diagnosis"

The owner UI stays in T1 (EV-019), but its **job** is now ruled, and it is not the operating surface.
The primary consumers are **agents** (configuration and diagnosis) and **services** (sending mail).
The owner's own use is occasional and falls into three named cases:

1. **Something has gone seriously wrong** — the "wtf is going on right now" read.
2. **Seeing what configuration exists** — the current routing picture, as a whole.
3. **Idle curiosity** — browsing the configuration for its own sake.

Plus, for mail specifically: **volumes**.

Consequences the build must carry:

- Routine operation is the **machine API**, not the UI. Route CRUD, application registration and
  credential lifecycle are things agents do; the UI showing them is secondary and may lag the API in
  capability without the product being incomplete.
- The UI is optimized for **reading under stress and reading out of curiosity** — the same surface
  serves both, because both are "show me what is actually true right now."
- **Agents diagnose too.** Every diagnostic view the UI offers needs a machine-readable equivalent,
  or agents are pushed into scraping a UI built for a human who is rarely there.
- This does *not* make the UI read-only — it makes it inspection-first. The owner's rare write acts
  are the emergency ones (F-04, F-10), which is exactly when clarity matters most.
