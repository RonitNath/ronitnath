# Phase 1 — Design brief + token port (sonnet styling leg)

Proves procedures/design.md on a real project.

Sources: legacy repo (nexus `~/dev/ronitnath`) `ui/src/styles/theme.css.ts`,
`atmosphere.css.ts`, `static/fonts/agency-bold.ttf`.

Scope:
- Write `docs/design.md`: codify the existing language — exact OKLCH tokens
  (bg 0.06/0.10/0.14 hue 240, fg 0.96/0.75/0.55, border 0.22, accent
  0.65 0.15 210, hover 0.72), Agency Bold display / system-ui body, radius
  0.25rem, bordered-pill buttons, columns 1200/760/500 (mobile 640), dark
  default + gold-stars light mode, `__rn_theme` cookie + pre-paint script.
- Poster-theming sub-language: what an event page MAY override (accent,
  atmosphere layer, display font) and MAY NOT (spacing scale, nav, a11y
  floors, status vocabulary).
- Port tokens into stage_2's `static/base.css` / `templates/_theme.html`
  structure (keep the FOUC block + CSP hash discipline in sync). Starfield/
  nebula as plain CSS. Copy agency-bold.ttf.
- Rebuild home page (name, tagline, socials) to match legacy feel.
- Run design/completeness.md audit; fix skeleton's unstyled surfaces
  (select, :focus-visible, ::placeholder, scrollbars, dialog) at token level.

Acceptance: brief exists; home + auth pages side-by-side screenshots vs
legacy site are indistinguishable in feel; completeness audit clean;
CSP hashes still valid (security_headers test green).
