# EV-045 — Owner rulings, design round 6 reactions (2026-08-06)

Provenance: ground-truth (owner verbatim, design gate session ②, round 7 instructions).

## Verbatim

> "A booking flow which is mid-progress (i.e. booking hold is made) but then a schedule exclusion lands - the agent should be notified, speak something to the effect of "sorry, my manager just notified me this slot is closed" and continue."

> "Round 6 is fine, just a few things: make the provider names buttons, selected is lightly shaded with their color, deselected is entirely transparent."

> "Separately, make a page which has a flow chart with potential call trees. This represents all the expected regular pathways for what the calling agent should be able to handle. Use symbolic logic, as there may be a number of cases which can be handled as "nonsensical answer" or similar, and you shouldn't make a new box per entry on the flow graph."

## Consequences

1. **Mid-call hold revocation** (→ PKT-02, PKT-06, call tree): a schedule exclusion landing while a hold is open must notify the in-call agent immediately; the agent acknowledges to the caller ("sorry, my manager just notified me this slot is closed") and continues the flow by re-offering slots. Holds are therefore revocable *during* the call, not just by TTL expiry — the exclusion wins, the call self-heals.
2. **Provider legend is a row of buttons** (→ PKT-06): selected = lightly shaded with that provider's own calendar color (quiet tint) with the name in the provider color; deselected = entirely transparent background. Round-6 rendering (plain colored words) superseded.
3. **Call-tree page** (→ flows addendum / COVERAGE): a design-gate page showing all expected regular pathways the calling agent handles, drawn as a flow chart using symbolic logic — shared guard nodes (e.g. any unintelligible / nonsensical / off-topic reply routes to one repair node) instead of one box per case.
4. Round 6 otherwise accepted ("Round 6 is fine").
