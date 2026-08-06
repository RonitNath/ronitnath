# EV-044 — Design round-5 owner rulings (gate session ②, round 6)

- Type: ground-truth (owner statements + one direct file edit, 2026-08-06)
- Source: design-gate session ②, corrections after round 5 (schedule desk family)
- Amends: PKT-01, PKT-03, PKT-05, PKT-06, PKT-10; COVERAGE §5

## Verbatim rulings

> "I also want a month view. For the full week view, the weekends should just not be there, which
> gives more space for the regular days. You have it here as all providers - but this doesn't show
> me how you'd represent the UI when there are providers with overlapping times. Also, if we're
> putting multiple providers on the same UI, they should be color coded, and listed at the top.
> This way, it becomes 'Select All' 'Deselect All' and you can select per-person. The spacing on
> the edit window was a bit off, so I fixed that. Also include next/prev buttons so you can cycle
> through time periods (e.g. next week). Also, let's ideate regarding the availability page. How do
> we design the interface which allows providers to configure which days they're open? What about
> exclusions, how will we manage that? For instance, taking a vacation, calling in sick, holidays,
> etc."

> "Let's exclude the magnification states from this design; doesn't need to be part of the base
> app. Also make the mobile, ultrawide, tablet, and light mode desktop views for active-call and
> call-review."

## The direct edit (owner fixed the popover in Penpot)

Measured from the file: "Saved just now" moved up from +204 to **+182** relative to the popover
top; popover height cut **236 → 211**. Everything above the meta line untouched. The offense was
dead air under the trailing meta: the actions→meta gap should be ~32px and the bottom pad ~14px,
not a half-row of empty panel. Rule extracted: **a panel ends where its content ends** — trailing
meta sits tight under the last row, no reserved space.

## Consequences

1. **Month view** joins the schedule view set: Day · 3 days · Week · Month · Agenda (PKT-06).
2. **Weekend columns die.** There is no workdays/full-week toggle; the week (and month) grid shows
   only the practice's open days, giving those days the space. Interpretation on record: *which*
   days are open comes from availability configuration, so a practice that works Saturdays gets a
   Saturday column — the calendar renders the configured shape, it doesn't offer a toggle.
3. **Multi-provider representation** (PKT-06): providers are color-coded and listed at the top of
   the calendar as the filter — per-person select plus "Select All" / "Deselect All" (this replaces
   the "All providers ▾" dropdown). Events carry their provider's color. Same-time events from
   different providers render side by side in the day column; the mock must show that case.
4. **Next/prev period controls** on every calendar view (‹ › cycling week/day/month/agenda range).
5. **Availability page ideation requested** (PKT-03): the interface must let providers configure
   which days they're open, and manage exclusions — vacation, sick days, holidays. Requirement is
   real now; interface design is under ideation, data model pending at the data gate.
6. **Magnification states are excluded from the design** — not part of the base app. The A− / A+
   control leaves the sidebar footer; no magnification variants are owed at the breakpoint layer.
7. **Breakpoint set named** (COVERAGE §5): mobile, tablet, ultrawide, and light-mode desktop views
   for active-call (PKT-10) and call-review (PKT-05). Mobile/tablet are redesigns of the frame, not
   squeezes; light desktop uses the canonical `style-brass.css` light aliases.
