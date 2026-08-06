---
id: EV-022
date: 2026-08-06
provenance: ground-truth
source: owner, OQ-2 resolution (portfolio session 2026-08-06)
---
"feature integrations are gated; most of the product can be self-serve or manually onboarded via a sales call. Some require a sales call."

Settles the self-serve boundary (refines EV-016): the product core is reachable self-serve OR
via a sales call — both are first-class onboarding paths, not a fallback. Feature
*integrations* (PMS connection and similar) are gated: some can be unlocked in either path,
some exist only behind a sales call. Design consequence: the product must work and be sellable
*before* any gated integration is connected (e.g. rinity-native scheduling before PMS sync),
and the console needs a notion of gated/locked capability that a sales conversation unlocks.
