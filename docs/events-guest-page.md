# The guest page, before its design pass

`/e/<slug>?l=<token>` is the product (docs/stories/events.md). The owner's
ruling is that it gets a Figma pass before it is final; this is what R4 built
in the site's own system, and what a designer would want to argue with.

## What is on it

One column, `max-width: 34rem`, centred, on the app's ground (no starscape —
the sky is the landing's). Top to bottom:

1. **Title** — display face, `--text-5`. A host-chosen colour token tints one
   2px hairline above it and nothing else; a poster URL, if there is one, sits
   above the title as a bordered image.
2. **Lede** — the window in the reader's own zone (server renders the event's
   zone, the browser re-renders in the viewer's), the place *name*, then the
   host and the guest's own name. `--text-2`, muted, two lines.
3. **Body** — markdown-lite the host typed, rendered to a fixed tag set
   (`src/features/events/markup.ts`).
4. **Where** — *only after yes*: the address and the calendar link. Absent
   from the HTML before that, not hidden in it.
5. **Who's coming** — first names, CSS-blurred until the viewer answers yes,
   people sharing a circle first, "and N more" past eight. `full` sits beside
   the heading when the room is full.
6. **The answer** — three words in bordered boxes (never pills), a plus-one
   number, a note, one verb. An open link asks for a name first.
7. **A closing line**: no account needed, come back to change your answer.

## Decisions a designer should revisit

- **The blur is a filter on first names.** Softening a full name would leave
  the surname in the source, so the redaction is in the data and the blur is
  only the look of it. Any design that wants recognisable-but-soft faces or
  initials has to say what the HTML carries.
- **No photographs of anyone.** There are no avatars in the model yet, so the
  list is text. A design that wants faces adds a resource.
- **The answer is three bordered words**, sized like buttons, with a radio
  inside each. The site's rule is no status pills; a segmented control that
  reads as one is the obvious temptation here.
- **One column, no card.** The page is a sheet, not a poster on a wall. The
  poster image is the only visual the host controls, and it currently sits
  above the title rather than behind it.
- **Colour tokens are four names** (`night`, `ember`, `gold`, `moss`) that tint
  a hairline. If colour should carry more of the page — a tinted ground, a
  coloured title — that is a token change, not a component change.
- **Type sizes are the site's five.** The title is the same `--text-5` as an
  operator page's heading; an invitation might want to be louder than that.
- **The order is fixed**: who's coming sits above the answer, so the social
  proof is read first. Partiful puts the answer first on mobile.
- **Nothing animates.** The design brief is +2 calm; a page that celebrates a
  yes would be a deliberate exception.

## Where the styling lives

`src/app/e/[slug]/guest.css` — every rule for this page, all of it against the
tokens in `src/app/globals.css` (radius `0.25rem`, borders over shadows, both
themes authored). The markup carries stable hooks a restyle can address:
`.invite`, `.poster`, `.lede`, `.body`, `.details`, `.who-list[data-blurred]`,
`.names .name[data-shared]`, `.more`, `.full`, `.answer`, `.answers`,
`.choice[data-chosen]`. No component holds a colour or a size of its own.
