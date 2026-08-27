# Two taste calls, as variants

Findings 11 and 17 of `review.md` are not defects: the code does exactly what
somebody meant it to do, and what it should do instead is a decision about how
the site looks. Leg F6 did not change either one. Each is written up here as
the two options and the exact token lines that would change, so picking one is
an edit and not an investigation.

Nothing here is applied. The repository is on option A in both cases.

## 1. `--radius` — corners (finding 11 / V7)

`design/interface-taste.md` §Color, shape says "no rounded corners"
(2026-04-03). `crates/ui/tokens.css:68` sets `--radius: 0.25rem`, read by four
rules in `tokens.css` (`:163`, `:212`, `:264`, `:289`) and three in
`static/site.css` (`:323`, `:422`, `:559`) — inputs, buttons, the focus ring
and the rail's active row. It is 4 px, which is small enough to be a deliberate
softening rather than an oversight, which is why it is a question rather than a
fix.

**A — keep 0.25rem (today).** A 4 px corner is below the threshold at which a
control reads as a pill; it is the difference between an input that looks cut
out of the page and one that looks placed on it. The ruling was written against
capsules and cards, and this is neither. Cost: the ruling has an exception
nobody wrote down, and the next surface that wants 6 px has a precedent.

**B — square everything.** One token, and the ruling holds with no exception to
remember. Every affordance in the interface then shares one silhouette with the
tables, which already have none. Cost: focus rings and the rail's active row
get slightly harder edges against text, and the `:focus-visible` outline is the
place it shows most.

```css
/* crates/ui/tokens.css:68 */
  --radius: 0.25rem;   /* A — today */
  --radius: 0;         /* B — the ruling, with no exception */
```

No other line changes: every corner in the tree already reads the token.

## 2. The wordmark's colour (finding 17 / V1)

"A wordmark is one colour" (2026-06-06). Today it is four, and the pair
inverts between themes:

| surface | name | "Isoastra" |
|---|---|---|
| `/` dark | `--hero-red` | `--hero-gold` |
| `/` light | `--hero-gold` | `--hero-red` |
| `/auth`, `/links/<t>` | `--fg` (white on dark, near-black on light) | — |

The landing's pair lives in `static/site.css` as `--hero-fg` and
`--company-fg`, set in four places: `:root` (`:24`, `:28`), the
`prefers-color-scheme: light` block (`:48`, `:55`), `:root[data-theme="light"]`
(`:68`, `:75`) and `:root[data-theme="dark"]` (`:87`, `:90`). The askama pages'
`h1` takes `--fg` from `static/pages.css:28` and has no hero token at all.

**A — the identity pair, inverted per theme (today).** Two brand colours,
swapped so whichever one the sky is not wearing carries the name. It is the
most alive the landing looks, and the swap is what keeps both halves legible
against a dusk gradient and a black sky. Cost: the name is not one colour, and
a reader who arrives on `/auth` from a link sees a third one — so the wordmark
is not a mark, it is a treatment.

**B — one colour everywhere.** `--hero-red` is the name on every surface and in
both themes; "Isoastra" is a link and drops to the muted foreground, which is
what it is. `/auth` and the claim page take the same red, so the mark is the
same object wherever it appears. Cost: on the light landing, red at
`oklch(0.63 0.235 27)` sits on a dusk gradient that is already warm at the
horizon — it needs the existing `--hero-shadow` to hold, and it loses the gold
that currently does that work.

```css
/* static/site.css — B replaces the four pairs with one */
:root                        { --hero-fg: var(--hero-red);  --company-fg: var(--on-dusk-muted); }
@media (prefers-color-scheme: light) { :root:not([data-theme="dark"]) {
                               --hero-fg: var(--hero-red);  --company-fg: var(--on-dusk-muted); } }
:root[data-theme="light"]    { --hero-fg: var(--hero-red);  --company-fg: var(--on-dusk-muted); }
:root[data-theme="dark"]     { --hero-fg: var(--hero-red);  --company-fg: var(--on-dusk-muted); }

/* static/pages.css:28 — the askama pages join the mark */
.page > h1 { color: var(--fg); }          /* A — today */
.page > h1 { color: var(--hero-red); }    /* B */
```

`--hero-shadow` and `--hero-muted-shadow` stay exactly as they are under B:
they are what makes any colour survive the light theme's gradient, and they are
already theme-conditional for that reason.
