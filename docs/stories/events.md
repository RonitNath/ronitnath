# Events — user stories

Owner: Ronit Nath as host. Test event: **"Board games and hanging out"**, Ronit's
apartment, Sunday 2026-08-30, afternoon. No theme, no stakes — it exists to
walk every path below once. Written greenfield against the model (parties,
groups, links, resources, commands, audit), not the current code.

Rulings that shape these (2026-08-27):

- **Paste-link is the general pathway.** The platform mints links; the host
  pastes them wherever they already talk to people. No outbound SMS/Telegram
  to guests in the first cut.
- **The host works through an AI coding agent in a terminal** — creating the
  event, and before/on the day telling it things about guests ("Sam is a maybe
  now", "Priya's bringing a cake"). The agent is a first-class actor of its own
  (`docs/stories/agents.md`): the host's authority, its own name in the record,
  no auth dance. The host then adjusts wording and copy by hand in the UI.
- **Partiful-like experience.** The guest page is the product. A blurred
  "who's coming" list is social proof — visible before answering, sharpened
  after — and it puts people from circles the viewer shares first.
- **Interface design is a deliberate, separate step** (Figma), gated before
  the guest page is built. These stories say what; the design says how it
  looks.
- Events is the first product beyond the platform itself: enabled at runtime
  on ronitnath, off on isoastra.

## People

- **Host** — Ronit, in the browser and through the agent.
- **Agent** — a coding agent in a chat, acting as Ronit.
- **Held guests** — people in Ronit's world, most without a login.
- **Strangers** — a friend's partner, someone forwarded the link.
- **Attendees, after** — everyone who came, once.

## A. Host — creating (mostly through the agent)

1. I tell the agent "board games and hanging out at mine, Sunday afternoon,
   invite the SF friends group plus Sam and Priya" and the event exists: title,
   time window, place (address hidden until yes), a paragraph, capacity if I
   said one, audience = a group + named people. Anyone I named who isn't held
   yet becomes a held person with whatever handle I gave (phone, email, or
   just a name).
2. The event is a thing I own; created is not published. I open it in the
   browser, fix the copy and the wording, pick a colour or a poster, and
   publish. The agent's version and mine are the same event with one history.
3. Publishing mints links: one **personal link per invited person** and one
   **open link** for "bring whoever". I copy them and paste them where I talk
   to people. I can see, per person, whether their link was opened.
4. I change things after publishing — time, text, capacity — and the guest
   page and calendar entries follow. Nobody is emailed; the page is the truth.

## B. Guest — the page

5. I open my link: the page knows my name, shows the event and the blurred
   list of who's coming — names and faces softened, people I share a circle
   with first, "and 6 more" — and three answers: yes / maybe / no, a plus-one,
   a note. Ten seconds, no account.
6. After yes, the address and the details behind it appear, the list sharpens
   (to whatever the host allowed), and I can add the event to my calendar with
   an entry that follows the host's edits.
7. I open the same link later and it remembers me; I can change my answer and
   my note until the host closes RSVPs.
8. From the open link I am a stranger: I give a name (and optionally a way to
   reach me), answer, and from then on I am a held person with my own link.
9. I never see: the guest list beyond what the host allowed, the address
   before yes, the host's other events, anyone's notes to the host.
10. If I later sign in, everything under my link — answers, attendance, photos
    — is mine, and the host sees one person.

## C. Host — leading up (through the agent and the browser)

11. Live tally: yes / maybe / no / unopened, plus-ones counted, notes visible.
    A ping to me when an answer changes.
12. I tell the agent about people and it records it: "Sam is a maybe, coming
    late", "Priya's bringing a cake", "Alex can't make it" — the guest's status,
    what they're bringing, and a **host-private note** per guest. The agent's
    changes appear in the audit as *the agent, acting as me*.
13. I broadcast one message to the yes+maybe set — it appears on their page
    (and in their calendar entry); pasting still works for everyone else.
14. I close RSVPs when full and reopen if someone drops; the link says so.

## D. On the day

15. The door view on my phone: who's expected, tap to mark arrived, who's
    still out with what they said, what people said they'd bring. Late answers
    still land. The open link as a QR on the fridge for the friend-of-a-friend.
16. I keep telling the agent things during the afternoon ("Jordan arrived with
    two people") and the door view reflects it.

## E. After

17. Attendees drop photos on the event; attendees see them, nobody else;
    EXIF stripped; the host can hide one.
18. Attendance is a fact on each person: "came to board games 2026-08-30". Next
    time, "everyone who came to the last three things" is a group in one tap.
19. The event closes: links stop accepting answers, the page stays, history
    stays under the event and under each person.

## F. The agent as an actor (cross-cutting)

See `docs/stories/agents.md` — an agent is its own party, delegated by the
host, with the host's authority exactly and its own name in the audit; enrolled
once per machine, no sign-in. For this event: the host says what to do in a
terminal, the agent runs `rn` commands, the guest page shows the host.

## What this asks of the platform

- Resources: `event` (title, window, place, copy, poster, capacity, RSVP
  state), `rsvp` (answer, plus-ones, note, arrived), `guest_note` (host-private),
  `bring` (what someone said they'd bring), `broadcast`, `photo`.
- Audience as relations to persons and groups; **personal links** (identify
  without authenticating, revocable, claimable) and an **open link** that mints
  a held person.
- The **blurred list**: the viewer's identity (from their link) → shared groups
  → ordering; blur/sharpen by RSVP stage and host setting.
- Redaction by stage (address behind yes; list visibility per host setting).
- Host notifications on answer changes (channel: Telegram to the host, later).
- Calendar entry per guest that follows edits.
- **Agents as parties** (`docs/stories/agents.md`): the agent credential is
  the one non-cookie principal `/api` admits.
- Phone-first host view for the day; guest page designed in Figma first.
- Attendance as a first-class fact feeding group creation.
- Events as a runtime-enabled product.

## Open

- Default list visibility after yes: full names, or still first-name-only?
- Plus-ones: count only for the first cut; naming them becomes a held person
  later.
- Host notification channel: Telegram bot ping to Ronit (carried from the old
  site) or nothing in the first cut.
