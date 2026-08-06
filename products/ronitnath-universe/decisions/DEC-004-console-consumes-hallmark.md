---
id: DEC-004
date: 2026-08-05
---
Chose **hallmark (`@isoastra/tokens`) as the admin console's design language** over deriving a per-project console language, on owner instruction at the design gate.

**Documented conflict, resolved deliberately**: `procedures/design/unified-tokens.md` carved ronitnath *out* of hallmark ("client products … keep per-project design languages"). The carve-out is about **client/guest-facing product surfaces**, which still holds: presence keeps the night-sky language (`docs/design.md`) and invite pages are per-invite bespoke (EV-014). The **admin console is an internal ops surface with an audience of one** — exactly ember's stated use ("every internal Isoastra surface: tools, consoles, operations"). So the console consumes the contract and the product surfaces do not. Context doc amended to say this precisely rather than leaving a contradiction.

**Identity**: `ember` (red-internal) is the taxonomy match for a console. `brass` (gold-external) is the plausible alternative given ronitnath.com's gold-accent heritage, so both are built for real as option boards at the design gate rather than argued in prose (`procedures/design.md`).

Consumes: scales.css (4px grid, restrained type ramp with 15px body floor, square-by-law geometry, hairline rules), fonts (ember: Fraunces display / Newsreader body), alias tokens only — never ramps. Pinned exact version, upgraded deliberately.
