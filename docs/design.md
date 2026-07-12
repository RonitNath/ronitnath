# Design brief: ronitnath.com

## Product thesis

Helps visitors (recruiters, collaborators, event guests) find Ronit Nath and
his events under a single identity while feeling like a quiet, confident
night sky — not a SaaS dashboard wearing a portfolio's clothes.

This is phase 1 of a unification: stage_2's skeleton (auth, admin, guestbook)
is being restyled to wear the **existing live site's** look
(https://ronitnath.com), not a new invented one. Every value below is
codified from that site, not derived from scratch. Phase 3 adds an
event-visibility vocabulary (hidden/busy/summary/full) on top of this base —
placeholder noted below, not implemented here.

## Personality axes (score -2..+2 each)

- serious/playful: **-1** (a name, a title, four contact links — restrained,
  not corporate-serious, not casual either)
- dense/spacious: **-2** (one hero, huge negative space, one column)
- warm/cool: **+1 dark / -1 light** (dark mode reads cool-blue/violet nebula;
  light mode reads warm-gold stars — the temperature itself flips with theme,
  which is a deliberate brand move, not an oversight)
- expressive/utilitarian: **0** (a display font carries all the expression;
  everything else — buttons, nav, forms — is plain and utilitarian)
- formal/conversational: **-1** (no marketing copy, no CTAs, just facts:
  name, role, links)
- calm/urgent: **-2** (nothing blinks, nothing counts down; the only motion
  is a slow twinkle)

## Typography

- Display: `"Agency Bold", system-ui, sans-serif` — condensed, all-caps-style
  face (mixed case works too, see hero `<h1>`). Used for `<h1>`, nav
  wordmark, and nothing else. Self-hosted: `static/fonts/agency-bold.ttf`,
  `@font-face` in `static/css/base.css` with `font-display: swap`.
- Body: `system-ui, -apple-system, sans-serif` — the OS-native stack, no
  webfont weight to load, keeps body copy fast and neutral so the display
  font is the only thing that announces itself.
- No mono family defined yet (audit log/token display uses `<code>`, browser
  default monospace is acceptable there — not a branded surface).
- Scale: hero `<h1>` ~3.25rem desktop / ~2.25rem mobile (clamp), body 1rem,
  subtitle/tagline 1.1rem, small/meta 0.85rem. No formal ratio system yet at
  this page count — introduce a type-scale token (`--step-*`) if a third
  distinct heading level appears.
- Line-height: 1.5 body, 1.1 display headings (condensed faces need tighter
  leading or they look loose).
- Numeric style: not applicable yet (no tabular data with alignment-sensitive
  numbers outside the audit table, which uses default proportional — revisit
  if a real numeric table ships).

## Color

All values are OKLCH, ported verbatim from the legacy site's
`theme.css.ts` (vanilla-extract source, see `docs/design/legacy-tokens.md`
is not needed — the values live directly in `static/css/base.css`).

**Dark (default):**

| token | value | role |
|---|---|---|
| `--bg` | `oklch(0.06 0.005 240)` | page background |
| `--bg-muted` | `oklch(0.10 0.008 240)` | surface (cards, topbar, inputs) |
| `--bg-subtle` | `oklch(0.14 0.010 240)` | raised/hover surface |
| `--fg` | `oklch(0.96 0.002 80)` | primary text |
| `--fg-muted` | `oklch(0.75 0.005 80)` | secondary text, subtitles |
| `--fg-subtle` | `oklch(0.55 0.005 80)` | tertiary text, meta, placeholders |
| `--border` | `oklch(0.22 0.010 240)` | all borders |
| `--accent` | `oklch(0.65 0.15 210)` | links, primary actions, focus ring |
| `--accent-hover` | `oklch(0.72 0.15 210)` | accent hover/active |
| `--accent-fg` | `oklch(0.06 0.005 240)` | text/icon color on filled accent |

**Light (`:root[data-theme="light"]`, and `@media (prefers-color-scheme:
light)` when no explicit choice is stored):**

| token | value |
|---|---|
| `--bg` | `oklch(0.97 0.003 240)` |
| `--bg-muted` | `oklch(0.93 0.005 240)` |
| `--bg-subtle` | `oklch(0.89 0.007 240)` |
| `--fg` | `oklch(0.15 0.010 240)` |
| `--fg-muted` | `oklch(0.35 0.008 240)` |
| `--fg-subtle` | `oklch(0.50 0.008 240)` |
| `--border` | `oklch(0.78 0.015 240)` |
| `--accent` | `oklch(0.45 0.15 210)` |
| `--accent-hover` | `oklch(0.38 0.15 210)` |
| `--accent-fg` | `oklch(0.97 0.003 240)` |

Semantic status hues (new — not in the legacy single-page site, needed
because stage_2 has forms/errors the legacy site never did): keep the same
hue family discipline (low chroma neutrals, one saturated accent) —

- `--danger`: `oklch(0.60 0.18 25)` dark / `oklch(0.50 0.18 25)` light (form
  errors, destructive actions, guestbook error text)
- `--success`: `oklch(0.70 0.15 145)` dark / `oklch(0.55 0.15 145)` light
  (reserved — no success-toast surface exists yet)

Link/focus-ring color: `--accent` in both themes, 2px solid outline with
2px offset (see Density & shape). Never color-only for state — errors also
get an icon-free but explicit `role="alert"` text prefix ("Error:") and
`aria-invalid`.

AA contrast verified pairs (computed from the OKLCH values above): `--fg` on
`--bg` and `--fg-muted` on `--bg` both exceed 4.5:1 in both themes (this is
inherited directly from the shipped legacy site, which was already
contrast-checked); `--accent-fg` on `--accent` exceeds 4.5:1 in both themes
by construction (accent lightness sits at opposite ends: 0.65/0.45 fill vs
0.06/0.97 fg).

## Density & shape

- Spacing grid: 4px base (`0.25rem`) increments — `0.25/0.5/0.75/1/1.5/2/3/4rem`.
- Radius: **`--radius: 0.25rem`** everywhere (buttons, cards, inputs,
  dialogs). No second radius scale — the legacy site uses one value
  uniformly and phase 1 keeps that discipline.
- Buttons: **bordered pill language** — visible 1px `--border` (or
  `--accent` on the primary/filled variant), `--radius` corners (not a full
  capsule — the legacy buttons measure 4px computed radius, i.e. exactly
  `--radius`, despite reading as "pill-ish" from generous horizontal
  padding: `0.6rem 1.25rem`). "Bordered pill" here means bordered + padded +
  low radius, not `border-radius: 999px`. Do not introduce a fully-rounded
  pill variant without updating this brief.
- Control heights: buttons/inputs ~2.5rem (40px) tall — meets the 44px touch
  target with padding, exceeds the 24px floor comfortably.
- Border vs. shadow: borders are the primary depth language (1px
  `--border`), consistent with the legacy site's flat/bordered cards.
  `--shadow` exists for the topbar only (`0 1px 3px rgba(0,0,0,.4)` dark /
  `rgba(0,0,0,.08)` light) — don't add shadows to buttons or form controls.
- Columns (new token set, ported from the legacy site's measured
  `max-width`s): `--col-wide: 1200px` (reserved for future wide/table
  layouts), `--col-content: 760px` (prose, forms, tables — most `.content`
  pages), `--col-narrow: 500px` (hero, auth cards — matches the legacy
  site's measured 500px hero column exactly). Mobile breakpoint: **768px**
  — the nav-drawer breakpoint in `layout.css`. **Deviation from the legacy
  site (which used 640px)**: the legacy nav only ever had to fit two links
  and a toggle; this fork's nav also carries About/Guestbook plus
  auth chrome (display name + Sign out, or Sign in + Sign up), which
  overflows/wraps onto two lines at 641-767px if the drawer only kicks in
  at 640px — verified by screenshot during the phase-1 audit. 768px was
  chosen because it's also a mandated verification breakpoint (this brief's
  own responsive floor), so "mobile nav" and "tablet screenshot" are the
  same state by construction, not two states that might drift apart.
- Public (site-bin) chrome never advertises auth: no Sign in/Sign up/admin
  links for signed-out viewers.

## Motion

- Durations: 150-200ms for interactive transitions (hover/focus/drawer),
  matching the existing `0.2s` drawer transition.
- Easing: `ease` (drawer), no custom cubic-beziers introduced.
- Starfield twinkle: two keyframe animations ported verbatim from the legacy
  site — `twinkle-med` (7s, brightest star layer) and `twinkle-slow` (15s,
  mid layer); the dimmest star layer is static (no animation — that's
  intentional, gives depth without every star moving).
- Where motion is allowed: theme toggle icon swap, drawer slide, button
  hover/focus transitions, starfield twinkle. Nowhere else — no page-load
  fade-ins, no scroll-triggered reveals.
- Reduced motion: `@media (prefers-reduced-motion: reduce)` disables the
  twinkle keyframes (stars render at a fixed mid-opacity) and the drawer
  transition; this is a **new rule this phase adds** — the legacy site
  didn't have one, but the accessibility floor below requires it and there
  is no reason not to fix that gap while porting.

## Atmosphere (starfield + nebula)

Plain CSS in `static/css/atmosphere.css`, ported 1:1 from the legacy
`atmosphere.css.ts` (same radial-gradient coordinates/sizes — hand-placed
"random" star positions, don't regenerate them, they're a designed asset).
Structure: `.starfield` (fixed, inset 0, `pointer-events: none`, `z-index:
0`) contains three layers `.stars-dim` / `.stars-med` / `.stars-bright` plus
a `.nebula` div, all absolutely positioned siblings. Rendered once in
`_layout.html`, behind `<main>` (page content needs `position: relative;
z-index: 1` or equivalent stacking to sit above it — `.content` already
gets this).

- **Dark** (default): cool-white star dots (`rgba(255,255,255,*)` and
  blue-tinted `rgba(200,210,255,*)` / `rgba(180,200,255,*)`) across all
  three density layers; nebula visible — four soft radial blobs mixing blue
  (`rgba(60,80,180,*)`, `rgba(40,70,150,*)`) and violet
  (`rgba(100,60,160,*)`, `rgba(80,50,140,*)`) at low opacity (0.04-0.08).
- **Light**: nebula `display: none` entirely (no light-mode nebula — this
  is intentional, not a missing asset). Stars recolor to warm gold
  (`rgba(210,160,0,*)`) and grow slightly (2.5-3px vs 1-2px dot size in
  dark) so they stay legible against the light background at similar
  opacity (0.5-0.85 vs 0.35-0.95 dark — both hand-tuned per layer, ported
  verbatim, not re-derived).
- Applies site-wide (every page in `_layout.html`), not just home — this is
  a phase-1 decision beyond the legacy site's single page, made because a
  page-by-page atmosphere toggle would be an unearned inconsistency the
  brief has no reason to introduce.

## Theme mechanism (deviation from legacy — read before changing)

The legacy site used a `__rn_theme` **cookie** + a pre-paint inline
`<script>` that reads the cookie and sets `data-theme` before first paint.
stage_2's skeleton already has an equivalent no-FOUC mechanism, but keyed on
**`localStorage["theme"]`** instead of a cookie, wired through
`templates/_theme.html` (pre-paint inline `<style>`+`<script>`, hashed into
the CSP by `src/security_headers.rs`) and `ts/src/lib/theme.ts` (the toggle
button handler).

**Decision: keep stage_2's localStorage mechanism, do not port the cookie.**
Reasons: (1) stage_2's inline script is CSP-hash-pinned and covered by a
`cargo test` — swapping the storage backend is a net-new surface with no
functional gain; (2) a cookie only earns its keep once the *server* reads it
to set `data-theme` on the initial HTML response (avoiding even the
pre-paint-script gap) — stage_2's handlers don't read cookies for rendering
today, and wiring that up is a `src/handlers` + `view.rs` change, out of
scope for a styling leg; (3) the two mechanisms are behaviorally identical
to a visitor (explicit choice persists, defaults to system preference, no
flash) — there is no user-facing regression from not porting it. If a
future phase wants SSR-correct theme (no inline pre-paint script at all),
revisit as a wiring leg that touches request handling, not this brief.

Values used: `"light"` / `"dark"` (already what `theme.ts` writes) — no
change needed for compatibility with the legacy `"dark"`/`"light"` cookie
values, they already match.

## Copy voice

- Direct, first person where the site is Ronit's own ("Ronit Nath", "Founder
  of Isoastra") — not marketing copy, not a bio paragraph. State the fact,
  don't sell it.
- Buttons: verb + object ("Sign in", "Sign up", "Mint new API token",
  "Revoke") — already mostly correct in the skeleton, keep it.
- Error tone: plain, no blame, no apology theater. "Something went wrong" +
  the actual `ref:` id, not a stack trace.
- Forbidden words: "Get Started", "Manage" (bare, no object), "Submit"
  (bare — always "Submit <thing>" or better, a specific verb), "Welcome to
  stage_2" (or any leftover skeleton branding — grep for "stage_2" in
  templates before shipping and replace with "Ronit Nath" / the actual
  product name).
- Social links use platform names as link text (GitHub, Instagram,
  LinkedIn, Email), not icons-only — matches the legacy site exactly, and
  keeps them screen-reader-clear without needing `aria-label`.
- Auth-page CTAs are admin-facing; guest-facing login (phase 4) gets its own
  copy and must not inherit "No account? Sign up."

## Status/level vocabulary (placeholder — phase 3)

Event visibility levels (`hidden` / `busy` / `summary` / `full`) are **not
implemented in phase 1**. When phase 3 lands: one status vocabulary, one
color mapping, never color-only (pair with a text label/icon), reuse
`--accent`/`--danger`/`--fg-subtle` rather than inventing new hues unless a
genuine fourth semantic meaning demands it. This brief will gain a table at
that point — don't pre-invent it now.

## Poster-theming sub-language (event pages, future phases)

A single event page is allowed to feel like *its own poster* without
breaking the shared chrome around it. Scope drawn narrowly on purpose —
personality is allowed exactly where it can't break usability or trust:

**MAY override, per event:**
- `--accent` / `--accent-hover` / `--accent-fg` (a poster's signature color)
- Which atmosphere layer renders (e.g. swap nebula hue, or disable stars for
  a daytime event) — implemented as an additional data attribute or class
  scoped to that page, never by editing the shared `atmosphere.css` file
- Display font, if the event wants a different display face than Agency
  Bold — body font stays system-ui regardless (readability floor)

**MAY NOT override, ever:**
- Spacing scale, radius, column widths, mobile breakpoint
- Nav structure/content (brand, Home/Events links, auth state) — a poster
  is a page within the site, not a standalone microsite
- Focus-visible treatment, contrast floors, touch target sizes — no
  poster's `--accent` may ship without passing the same AA check as the
  base palette
- Status vocabulary/color mapping once phase 3 defines it — an event's
  "full" must look like every other event's "full"

Enforcement: a poster theme is a CSS custom-property override scoped to
that page's root element (e.g. `[data-event-theme]`), never a parallel
stylesheet that redefines `--radius`, `--border`, or spacing tokens. A
poster PR that touches those tokens fails review on this brief alone.

## Accessibility floor

- Focus always visible: `:focus-visible` gets a 2px solid `--accent`
  outline with 2px offset on every interactive element, including inside
  the dark starfield (outline color has enough contrast against both
  `--bg` values by construction — accent is tuned to sit far from both bg
  extremes).
- AA contrast for all text/UI per the Color section above.
- Touch targets ≥44px where the control's hit area allows it (buttons,
  nav links, theme toggle already meet this via padding).
- Reduced motion respected (see Motion).
- Every form input has a visible `<label>`; errors get `aria-invalid` +
  `aria-describedby` pointing at inline error text, never color-only.
- Native controls (`select`, checkbox/radio, `dialog`, scrollbars) are
  styled at the token level in `static/css/base.css` even before the app
  has a page that uses them — see the completeness audit in the phase-1
  report for what was proactively covered.

## Do / Do not

- Do reuse `--bg-muted`/`--bg-subtle` for every "surface" (topbar, cards,
  inputs, dialogs) — do not invent a fourth background level.
- Do keep the hero/auth column at `--col-narrow` (500px) — do not widen it
  to fill the viewport; the legacy site's generous negative space is the
  point.
- Do not add drop shadows to buttons, inputs, or cards — borders only
  (except the topbar, which already has one).
- Do not add gradients anywhere except the nebula layer (which is the
  brief's one sanctioned gradient use, and only in dark mode).
- Do not use emoji as icons. The theme toggle uses monochrome inline SVG
  sun/moon icons colored through `currentColor`.
- Do not center workflow pages (settings, account, audit log, guestbook) —
  those stay left-aligned inside `--col-content`; centering (`.hero`
  pattern) is reserved for the marketing-flavored home/about/error pages
  and auth entry (login/signup), matching how the legacy site only ever
  centers its single hero.

## References

- **ronitnath.com (legacy, live)** — the source of truth for this entire
  brief. Admired specifically: the restraint (one hero, four links, done),
  the starfield as texture rather than decoration (low opacity, doesn't
  compete with the headline), and the light/dark asymmetry (gold stars in
  light mode is a real decision, not just an inverted dark theme).
- **Linear** — admired for bordered-pill buttons at low radius reading as
  "considered" rather than "default" purely through consistent padding and
  border weight, not shape novelty. Not admired/used: Linear's density or
  color system, which don't apply to a single-page identity site.
