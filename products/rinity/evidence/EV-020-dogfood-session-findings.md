---
id: EV-020
date: 2026-08-06
provenance: inference
source: authenticated dogfood of https://rinity.isoastra.com (staging, digest 3b7182b4…) as agent-ops via headless OIDC; screenshots in EV-020-assets/; directed by EV-012
---
What logging into rinity and using it is actually like, judged as a prospective practice
(EV-012's frame). Verifiable against staging and the EV-020-assets screenshots.

## What exists — and it is good

The authenticated console is **one screen: the "Rinity schedule desk"** — a live calendar for
the one configured office (Rose City Dental Arts · Pasadena, demo adapter, fixture patients).
Within that screen the craft is real:

- Week / Month / Day / **Providers** views; the Providers view shows weekly provider load
  (visits, minutes booked, openings, % scheduled) per provider (`rinity-04-providers.png`)
- Quick-create: patient search (fixture-labeled, DOB-disambiguated), provider/time/duration
  prefilled, booking lands through the real adapter into the system of record and the
  calendar updates live ("Live calendar updated", SSE) — verified end-to-end by booking and
  then cancelling a visit (`rinity-02`, `rinity-03`)
- Full edit on an existing visit: patient, date, start, duration (15–120m), provider,
  cancel — and **cancel offers Undo**
- Keyboard model (arrows, T, Enter, Esc, undo), office-local timezone handling, a mini-month
  navigator. Design language is a warm cream + brick-red, serif-headed, calm and legible.

Small defects: quick-create defaults to the **first slot of the visible week — a past slot**
(booked Monday 8 AM when today is Thursday) with no past-time guard; favicon 404; the
CF-injected beacon is CSP-blocked (known doctrine caveat, correct but noisy).

## What does not exist

Every other route 404s (`/patients`, `/calls`, `/settings`, `/admin`, `/offices`,
`/onboarding`, `/billing`, `/signup`). Concretely absent from the product a customer touches:

1. **Voice. Anywhere.** No calls, no transcripts, no recordings, no agent configuration, no
   phone numbers, no "what did the agent say to my patient" — the flagship's namesake
   capability has zero surface. The engine exists (EV-011) but rinity has no seam to it
   (EV-010 §3), and nothing in the console admits voice exists.
2. **Any second screen.** No patient list outside the booking modal, no appointment list, no
   day-sheet, no messages/tasks, no reporting.
3. **Self-serve** (EV-016): admission is a deploy-time env allowlist keyed to one subject; no
   signup, no onboarding, no billing. Org/office creation exist only as raw APIs.
4. **A front door**: the signed-out page is an unstyled box — one sentence and a Sign in
   link (`rinity-05-signed-out.png`). A prospective customer learns nothing and can do nothing.

## The reading

**Rinity today is a receptionist's tool, not a receptionist.** It is a polished human-operated
schedule desk — the artifact a front-desk person would use — while the bet (EV-013, "replace
the receptionist") requires the product to *be* the thing answering the phone, with the
console as the practice's window into what their agent did. The experience a real business
expects — sign up, connect a line, configure the agent, hear it handle a call, see the
outcome land on this (genuinely good) calendar, pay — exists at exactly one of those six
steps. The calendar is the strongest possible foundation for "see the outcome land"; every
other step is unbuilt surface.
