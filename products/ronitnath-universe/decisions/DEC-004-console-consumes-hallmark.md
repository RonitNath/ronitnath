---
id: DEC-004
date: 2026-08-05
---
Chose **hallmark (`@isoastra/tokens`) as the admin console's design language** over deriving a per-project console language, on owner instruction at the design gate.

**Documented conflict, resolved deliberately**: `procedures/design/unified-tokens.md` carved ronitnath *out* of hallmark ("client products … keep per-project design languages"). The carve-out is about **client/guest-facing product surfaces**, which still holds: presence keeps the night-sky language (`docs/design.md`) and invite pages are per-invite bespoke (EV-014). The **admin console is an internal ops surface with an audience of one** — exactly ember's stated use ("every internal Isoastra surface: tools, consoles, operations"). So the console consumes the contract and the product surfaces do not. Context doc amended to say this precisely rather than leaving a contradiction.

**Identity — RESOLVED 2026-08-05: `ember`.** Owner picked it at the design gate after seeing both built for real as option boards ("Pick ember"), confirming the taxonomy match — a console is an internal ops surface. `brass` (gold-external), the plausible alternative given ronitnath.com's gold-accent heritage, is not taken; presence keeps the gold heritage on its own night-sky language, which is where that heritage actually lives. The option boards did their job and the options page can retire (`system/design-gate.md`: options pages exist only while a decision is live).

Consumes: scales.css (4px grid, restrained type ramp with 15px body floor, square-by-law geometry, hairline rules), fonts (ember: Fraunces display / Newsreader body), alias tokens only — never ramps. Pinned exact version, upgraded deliberately.
