# Design brief: rn-site (ronitnath.com)

## Product thesis

Helps visitors (and the site's owner, on internal surfaces) see who Ronit is
and — behind capability guards — inspect the application's own data, under the
constraint of a fully self-hosted single binary, while feeling like a quiet
night sky: calm, precise, a little cosmic.

## Personality axes

- serious/playful: **+1 playful** — the starscape and mini-globe are the point,
  but they stay in the background layer, never in the content.
- dense/spacious: **-1 spacious** on public pages, **+1 dense** on `/manage`
  (operator tables earn density; the sky does not follow them indoors).
- warm/cool: **+2 cool** — everything sits on hue 240 (night blue); the two
  warm accents (red 27, gold 80) are reserved for the hero identity pair.
- expressive/utilitarian: public **+1 expressive**, internal **+2 utilitarian**.
- formal/conversational: **0** — plain declarative copy, no exclamation marks.
- calm/urgent: **+2 calm** — nothing blinks, nothing counts down.

## Typography

- Display: `"Agency Bold", system-ui, sans-serif` (`--font-display`) — hero and
  page titles only.
- Body: system-ui stack (`--font-body`). No webfont for body text; the site
  must render instantly from one binary.
- Mono: `ui-monospace, monospace` — identifiers, hashes, capabilities, all
  table values that are machine-given (UUIDs, timestamps).
- Tabular numerals for counts and timestamps in tables
  (`font-variant-numeric: tabular-nums`).

## Color

All values OKLCH, defined once in `public/css/starscape.css` (`:root`) for the
app shell; standalone server-rendered pages (guard pages, `/manage`) inline the
same values — same numbers, no drift.

- Ground: `--bg oklch(0.06 0.005 240)` dark / `oklch(0.97 0.003 240)` light.
- Ink: `--fg oklch(0.96 0.002 80)` dark / `oklch(0.15 0.010 240)` light.
- Muted ink: `--fg-muted`, `--fg-subtle` per starscape.css.
- Accent (links, focus, active nav): cyan `oklch(0.65 0.15 210)` dark /
  `oklch(0.34 0.14 215)` light. Hover shifts lightness, never hue.
- Identity pair (public hero only): red `oklch(0.63 0.235 27)` and gold
  `oklch(0.80 0.135 80)`. **Never used on internal surfaces.**
- Status hues on `/manage`: `active` = accent-tinted, `disabled/suspended` =
  `--fg-subtle`. No red/green traffic lights — status is a word, color is a
  whisper.
- Dark and light are both authored (never inverted); `color-scheme` declared.

## Density & shape

- Radius: `--radius: 0.25rem` everywhere. No pill buttons, no circles except
  the theme toggle icon.
- Public spacing is generous (hero-centric). `/manage`: 0.5rem cell padding,
  table rows ~2.25rem, section gap 2rem, max content width 72rem.
- Borders over shadows: 1px `--border` lines and glass fills
  (`--surface-glass`) — the aesthetic is a pane of glass over the sky, not
  paper floating above it.

## Motion

- Public sky animates; UI chrome does not. Transitions ≤150ms, ease-out,
  hover/focus only. `/manage` has no animation at all. Reduced motion is
  respected globally (the starscape stops repainting).

## Copy voice

- Declarative, lowercase-calm, no exclamation marks. Buttons are verb+object
  ("Log in", "Sign out"). Errors say what happened and what to do next.
- Auth failures are deliberately uniform: "Authentication failed." — never a
  field-level hint (security property, not a copy choice).
- Table headers are human words ("Created", "Role"), not schema names.

## Do / Do not

- Do keep internal ids inside the server: `/manage` shows `public_id`s, emails,
  and names — never the integer join keys.
- Do redact secrets structurally: password hashes render as the algorithm name
  only; session tokens as an 8-char digest prefix. The full value must not be
  in the HTML at all.
- Do give every table an empty state that says what the table is and why it
  might be empty.
- Do not put the starscape, globe, or warm hero colors on internal pages.
- Do not use browser-default (UA) styling anywhere — tables, buttons and links
  on standalone pages are styled explicitly.
- Do not use emoji as icons.

## References

- The legacy ronitnath.com "universe" site — the atmosphere/starscape assets
  are ported 1:1 and are a designed asset; do not regenerate star positions.
- Linear — dense, restrained operator tables: hairline dividers, muted
  headers, mono identifiers; not its palette.
- SQLite's own documentation pages — content-first, zero chrome, instant.
