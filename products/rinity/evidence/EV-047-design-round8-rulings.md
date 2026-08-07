# EV-047 — Design round 8 rulings: settings re-scrape, capsules accepted, availability + round-5 positions accepted

- date: 2026-08-06
- source: owner, design gate session ② round 8 reactions (ground truth, verbatim below)
- status: accepted

## Owner verbatim

> Note on the scrape: this should also be available via settings, in case the user updates their practice information, and wants to use this path to update their settings automatically while already a client with history. lgtm on above. I'll accept capsules, your availability ideation was fine, round 5 was fine.

## Consequences

1. **Scrape is not onboarding-only.** The website-scrape path gets a second entry point in console settings: an existing client — with history — who updates their practice website can re-run the scrape to update their settings automatically (PKT-07 + PKT-03).
2. **Re-scrape merges against history** (inference, consistent with EV-043's do-first-then-ask rule): on a re-scrape, findings that contradict existing configured facts — including hand-edited knowledge entries and settings — land in the approval queue rather than being silently applied; only uncontradicted findings are applied directly. Provenance per entry (EV-041) is what makes this reviewable.
3. **Round 8 accepted** ("lgtm on above"): the EV-046 packet deltas and the rebuilt call tree stand as-is.
4. **Capsules accepted.** The base 1440 queue board's OUTCOME chips and NEEDS ATTENTION badges stay as tinted capsules — this resolves the round-6 flag in favor of capsules and scopes the cross-product "a state is a word in its own colour, never a tinted capsule" law: for rinity's call-outcome chips and needs-attention badges, capsules are the accepted form. The F-4 breakpoint boards that used colored words are to be aligned back to capsules for consistency with the accepted base.
5. **Availability ideation accepted** ("your availability ideation was fine") — mocks proceed on the ideated shape: per-provider weekly template whose union of open days is the *cause* of which columns the calendar renders; exclusions as a dated "Time away" list (who / date range / reason as a word in its own colour), not a grid; entries that collide with booked visits name what was witnessed ("2 booked visits fall in this range →") and queue for reschedule rather than auto-cancelling; standard holidays seeded by the onboarding scrape as applied-but-reviewable items. The open question (per-provider from day one vs practice-level) was not contradicted, so the multi-provider position stands: per-provider from day one.
6. **Round-5 positions accepted** ("round 5 was fine"): provider columns in day view, and the office picker ("Cedar Street ▾") under the practice name in schedule sidebars.
