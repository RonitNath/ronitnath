# DEBATE-002 — flows, data model, design decisions

Generated from `DEBATE-002-flows-and-data-model.yaml`. Panel and rules: `system/debate-heuristics.md`.
Eight lenses on four opposed axes, `openai-codex/gpt-5.6-sol:high`, 68 claims, 8 staked, one
inference-only. Four lenses ran on restricted context (evidence + flows + code, no pitch, no
decisions) so they could not be anchored by the reasoning they were testing.

**The judge is compromised.** Claude wrote FLOWS-001, DEC-005, DEC-006 and packets 11/13/14, all of
which are under debate here. This memo is an input the owner reviews, not an adjudication.

## What the round actually caught

Five findings were reached independently by lenses whose mandates agree on nothing else. That
independence is the evidence — no shared instruction explains them.

1. **There is no release entity.** Nothing binds an event row to the code that renders it, so an event
   can be created, published or activated in a state its public route cannot render. Cohesion,
   variance, agent-autonomy and failure-realism all arrived here separately. F-2 already flagged the
   deploy seam as open; what the panel adds is that no *ordering* fixes it — the entity is missing.
   **→ EventRelease plus a compare-and-swap activation pointer; archived events pinned to a release.**
2. **PKT-13's SSE guarantee is not implementable.** Gap-free `Last-Event-ID` reconnect cannot work over
   an in-process bus or `LISTEN`/`NOTIFY` — both are fire-and-forget, and DEC-006 names only those two.
   Either SSE becomes a notification that something changed against durable revisioned state, or a
   durable change log with a monotonic sequence is required. **The packet acceptance is wrong as
   written either way**, and I wrote it.
3. **Last-write-wins on copy destroys wording irrecoverably** — and "surfaced" is a visual condition,
   so an API agent receives success for a write that silently clobbered the owner. Needs a revision
   per value and an optimistic precondition.
4. **The copy counter cannot mean "invited."** Clipboard success, server acknowledgement, app switch
   and actual sending fail independently. The owner's need is real; the label is a lie. It must say
   *copied*, and it needs an idempotent recovery path.
5. **Nothing records attendance.** F-9 promises to show whether someone showed up; no flow produces
   that fact, and deriving it from RSVP status makes the directory lie. Invitation, response and
   observed attendance are three facts. **→ a new flow, F-11, closing an event.**

Items 2–5 are defects in artifacts I authored, confirmed by lenses that never saw my reasoning.

## The collisions — decisions, not defects

**How much of presentation lives in the database (C-6 ↔ C-7).** Minimalist: six tables, identity stays
code behind a renderer key, no composition rows — the cheapest thing today's evidence forces, and it
preserves DEBATE-001 C-6. Archivist: then a Postgres dump cannot reconstruct what any invite page
showed, ever. This attacks the ruling the whole product shape rests on. The bet exists because data
about real people survived three codebases badly; the archivist's point is that under
identity-as-code the *pages* don't survive at all.

**Where the per-event extension boundary sits (C-8 ↔ C-9).** Cohesion: an extension may register
components and theme assets but must not add routes, tables, authorization, RSVP semantics or SSE
topics — otherwise "per-event admin panel" means N applications sharing a database. Variance: events
already needed exactly that, and it cites a real migration (`0020_segment_paid_attended`) and a real
admin media route from the predecessors. **Variance carries the better evidence here.** A boundary
that forbids per-event persistence reproduces the thing that forced the rebuilds.

**The base screen (C-10).** Three lenses object to the decision just taken, each for a different
reason: it is an owner-only dossier including people who never opened their link (guest-dignity); it
costs a navigation to reach the event in progress (operator-under-load); the invitee list is the
surface that does real work (minimalist). None of them can weigh these against why it was chosen —
the cross-event collapse is precisely what three codebases destroyed.

**Guest exposure (C-11).** Two concrete things. A forwarded personalized link lets whoever opens it
read *and overwrite* the intended invitee's answer, because F-7 binds every return through a link to
one identity — that is a correctness bug, not a values question. Separately, recording `uses` and
`last_used_at` on every capability GET makes opening an invitation a behavioural record the guest
never sees. Nothing in the artifacts argued the guest's side before this round.

**Retention against deletion (C-12).** The archivist and guest-dignity lenses each flagged this
*against their own mandate*, which is the panel working correctly. Preserving revoked URLs, notes and
superseded identities is exactly what a guest believes revocation removed. It decides F-10's merge
and archive semantics — and failure-realism adds that predecessor foreign keys cascade-delete
attendance, so a naive delete destroys the data this bet exists to preserve.

## Deferred as assumption

**Unattended agent creation (C-13).** Assume the human confirmation step stays. Agent-autonomy is
right that F-2/F-4 cannot run unattended, but two other lenses argued against their own mandates that
machine-gating every visual experiment would strangle what made past events distinct. Cheap to
reverse. What survives as real work: PKT-04 needs idempotency beyond create, an atomic manifest
write, an optimistic revision precondition, a stable machine error schema, and converged-state
readback.

## Escalated — one consolidated round for the owner

1. How much of presentation must the database hold? (C-6 ↔ C-7)
2. Where does the per-event extension boundary sit — may an event own tables, routes, commands? (C-8 ↔ C-9)
3. Base screen: keep the people directory, given three lenses object? (C-10)
4. Do guests get disclosure and a correction/deletion route; do we record link opens? (C-11)
5. Retention against deletion for merge and archive. (C-12)

## What would change the recommendation

The round used a single model family, so decorrelation was structural — different mandates, forbidden
moves, asymmetric context — not model-diverse. The convergences are strong evidence *because* the
mandates conflict; the collisions are the panel working. A second family on the two escalated data
questions would be worth its cost before the data gate commits.
