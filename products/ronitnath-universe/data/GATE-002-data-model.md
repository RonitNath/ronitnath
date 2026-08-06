---
gate: data (session ③ of PITCH-001's 5-session bound)
product: ronitnath-universe
pitch: pitch/ronitnath-universe-1
date: 2026-08-06
status: SIGNED OFF 2026-08-06 — domain model and seams approved; four escalations answered, one recommendation reversed, one new decision produced
artifact: `universe-ronitnath` repo, `changes/pitch-1/data-model.md` (the brief itself lives with the code — see "Why the brief is in the code repo")
---

# Data gate — domain model and seams

Session ③ spent. Two sessions remain in the bound: ④ working-software inspection, ⑤ pickleball-test
acceptance on the real domain.

## What the brief covers

The five sections `system/data-gate.md` requires: domain model (twelve platform tables plus a
reserved `ext_` namespace, id discipline, invariants, lifecycle), access patterns (the seven real
paths and the one projection they justify), interfaces and seams (HTTP/SSE/agent API, and why no
WebSockets), operational posture (streams against rolling deploys, additive-only migrations, backup,
caching), and peripheral services (one, and it is a file rather than a service).

## What the gate caught

**A contradiction nobody had noticed.** PITCH-001 §1, PKT-01 and DEC-006 all named a PostgreSQL seam.
The deployed reality is hiqlite, by an owner override issued the same day — when the store held one
disposable table, so it had never been weighed against the pitch's own words. The data gate was the
first place both documents were read together. Resolved as **DEC-013**: hiqlite for product data,
scoped to this product, with six rules the build must now follow (additive-only migrations first
among them).

**A counter that counts the wrong thing.** Escalation 4 asked whether session records should store a
coarse location. The owner answered that, then volunteered a defect in a different table: link
previews on iMessage and Instagram fetch invite URLs server-side, so DEC-009's per-GET `uses` counter
marks a link opened before a human sees it — *"over instagram meta crawls it like 11 times."* Every
"who has looked" surface was built on that number. Resolved as **DEC-014** (EV-029): raw fetches and
confirmed opens become two facts, and the confirmation is free because the SSE stream a guest page
already holds is something no preview fetcher will open.

**One recommendation reversed.** The brief argued archiving a person should leave their links alive.
The owner ruled it should revoke, and he is right — archiving someone while leaving them a working
URL contradicts the act in the only externally visible way (EV-028). What survives of the objection
is mechanism: an explicit update in the archive transaction, never a foreign-key cascade.

## Decisions and evidence produced

| Record | |
| --- | --- |
| **DEC-013** | hiqlite is the data seam (supersedes the pitch's PostgreSQL references) |
| **DEC-014** | Fetched is not opened (amends DEC-009) |
| **EV-027** | "hiqlite" |
| **EV-028** | "It should revoke." |
| **EV-029** | Raw opens are a weak signal |
| **EV-030** | Self-host geolocation; IP matters later for anti-bot |

The other seven deferred design-gate questions were reviewed and accepted as recommended
(*"I reviewed the open questions, lgtm"*); they are answered in the brief's §6 with reasoning.

## Loop-back check

`system/data-gate.md`'s loop-back rule: iterating here may reveal additional UI surfaces, which go
back through the design gate as layers 3–4. **None did.** Two boards need rebuilding with corrected
values — F-5's invitee list column becomes *opened / never opened*, and F-13's session rows become
buildable exactly as drawn now that geolocation is in — but both are existing surfaces with changed
content, not new ones. No return to the design gate.

## Packet deltas that follow

PRs against `changes/pitch-1/packets.md`, listed in the brief's §7: PKT-02/12/14 corrected for
DEC-014; PKT-13 gains exact reconnect and the guest-tier stream; PKT-09 gains backup-before-first-write
and the monthly geoip refresh; PKT-01/18 gain throttled `last_seen_at` and the `IpLocator` seam;
PKT-12 gains archive-revokes.

## Why the brief is in the code repo, not here

Asked at the session, and worth writing down because the split is not obvious.

`system/data-gate.md` specifies the location — `changes/<pitch-tag>/data-model.md` in the code repo —
and the reason is the README's four kinds of truth. Portfolio holds **product intent** (pitches,
briefs) and **durable records** (evidence, decisions). The code repo holds **delivery state**, and a
schema spec is delivery state: it churns with the code, and the migration that implements it should be
reviewable in the same diff as the document that specifies it. Split across repos, every schema delta
becomes a cross-repo PR pair, and the two halves drift.

**The design gate works the same way and nobody noticed** — its artifact (the Penpot boards and the
generators that build them) lives in the code repo at `design/penpot/`, and only the gate record lives
here as `GATE-001-console.md`. This file is the missing half of that symmetry: the data gate had an
artifact in the code repo and no record here. Now it has both.

Rule of thumb: **the artifact goes where it churns; the ruling goes where it accretes.**
