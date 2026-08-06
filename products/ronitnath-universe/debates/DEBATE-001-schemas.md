# DEBATE-001 memo — generated from DEBATE-001-schemas.yaml

> Trigger: T3. Scope: minimum identity schema + capability links; event page-doc/module schema; agent-API surface. Inherited OQ-4/5/6 — all closed below. **Pilot caveat**: single-context debate (proposer/red-team/synthesis passes by one model); independent multi-agent debate deferred, visibly.

## Options considered

- **Identity**: full platform-scope model (projects/roles/grants day one) vs **minimal cut** — identity, account, credential, auth_factor, session, capability_link, audit, with guests as account-less identity rows (C-1).
- **Page model**: fixed template vs **modular page doc** (C-5); style identity as props-only theme vs **bespoke per-event code as the normal path** (C-6).
- **Agent access**: agent identity rows vs **owner-minted named bearer tokens** (C-3).
- **Archive longevity**: render-compatible forever vs **freeze-on-archive** (C-7).

## Rejected, and why

- *Fixed template* — the three past events differed on five real axes; a template repeats the three-rebuild failure (C-5 ← EV-013).
- *Props-only theming* — two of three past visual identities required bespoke code; a props theme could not have produced the fireworks or starfield (C-6).
- *Grants/roles day one* — no second account exists during the bet; additive migration later; guarded by a single `authorize(actor, action)` seam so deferral doesn't smear owner-assumptions across handlers (C-2).
- *Agent identity rows* — owner ruling; named tokens give audit + revocation without an identity model for machines (C-3).
- *Legacy URL continuity* — moot; site dormant, nothing shared (C-12, closes OQ-6).

## Recommendation (surviving cluster)

Minimal identity cut with account-less guest identities (C-1); encrypted wire-ids from the first table, links/sessions as bearer tokens (C-4); owner-minted named+scoped API tokens as the agent surface (C-3, closes OQ-5); modular page docs validated per-module at write (C-5, C-7-mitigation); per-event style identity written as code by the creating agent, content in the doc (C-6); cut-one modules: hero, rich body, schedule (flat + segments), RSVP/status, entry instructions, style wrapper (C-8); **import of all three past events as the data-model acceptance test**, export as its inverse, app.db snapshot before cutover (C-11).

## What would change it (the deferred assumptions)

- **C-2**: a second account arriving during the bet → grants land immediately.
- **C-7**: if archived events must stay live/editable, freeze-on-archive is wrong → prop-schema versioning becomes real work.
- **C-9**: a paid/capacity event before the next bet → segment bookkeeping returns.
- **C-10**: guest activity resuming → notifications decision reopens (OQ-4 stays deferred, not dead).
