---
gate: data (session ③ of PITCH-001's bound)
product: rinity
pitch: pitch/rinity-1
date: 2026-08-06
status: SIGNED OFF 2026-08-06 (EV-049 — all six escalations ruled, adopted-by-default positions bound). Session ③ closes; schema changes are henceforth deltas with reasons.
artifact: rinity `changes/pitch-rinity-1/data-model.md` (authored 37667ee, rulings folded in at sign-off) — the artifact goes where it churns; this ruling record accretes here.
---

# Data gate — rinity

## What the brief covers

The five spec'd sections grounded in the as-built catalogs (EV-010 control plane, EV-011
audgent) and every CONSTRAINTS-pending line across PKT-01..PKT-16: the product-owned domain
model the accepted design surfaces require but EV-010 shows does not exist (persons with the
phone index, calls/matters/grades/summaries, the DEC-002 hold ledger made concrete, versioned
config documents superseding the dead RF-16 overlay, scrape tasks, run labels, run costs,
audit, ladder + compliance state); the hot paths (pre-call phone→person→summaries is the only
rinity call on a call's critical path); the seams (adapter v0.3 poll-first, engine
service-principal traffic both ways plus exactly three narrow engine additions — office-ledger
API with revoke push, read-only listen tap, run labels — SSE to the console with WS only for
listen audio, id.rinity.com as a second OIDC provider); operational posture (in-process
DB-backed tasks, rinity deploys leave in-flight calls untouched); and peripherals with their
failure behavior, including the explicit "no outbound SMS exists in tier 1".

## What the gate caught

1. **The store decision** — the brief recommended SQLite-stays; the owner overrode to
   **hiqlite** (DEC-003), aligning both products on the pilot's DEC-013 store and keeping the
   multi-node path open without a migration.
2. **The grader has a name now**: gpt-5.2 via the org's OpenAI credentials in the secrets
   store; execution home stays engine-side (AC-13 post-call analysis extension) — rinity holds
   no model credentials. Interpretation flagged in EV-049 in case the owner meant otherwise.
3. **Media retention flipped to indefinite** — the proposed 90-day default was the wrong
   instinct for a practice's call history; recordings and transcripts persist while the office
   is active, and the deletion-request path is the only eraser. Everything else in the
   retention block stands as proposed.
4. **Hold backstop tightened to 5 minutes** — the owner cut the proposed 15; release-at-call-end
   stays the primary semantics, so the backstop only covers a lost call-end event.
5. **Twilio confirmed, breadth deferred honestly** — "more later" is recorded as named later
   work, not a silent cap.
6. The design gate's promises all resolved to schema without a loop-back: **no new UI surfaces
   emerged from the data model.**

## Decisions and evidence

| Ruling | Record |
| --- | --- |
| Store = hiqlite, DEC-013 rules carried | DEC-003, EV-049 §1 |
| Grading/summaries = gpt-5.2, org OpenAI creds, engine-side | EV-049 §2 |
| Artifacts = reference + 15-min tokened proxy, no copies | EV-049 §3 |
| Carrier = Twilio tier-1, more later | EV-049 §4 |
| Retention = indefinite media, 30 d test artifacts, deletion path | EV-049 §5 |
| Holds release at call end, 5-min TTL backstop | EV-049 §6 |
| Adopted-by-default tail (schemas, sync, transfer, ladder, tap) | EV-049 closing ¶ |

## Loop-back check

None. The incomplete-contract queue, compliance checklist, ladder and scrape-review surfaces
were already designed or packet-named; grading moving engine-side changes no pixel; the
availability v2 surface already models provider_hours/time_away.

## Packet deltas that follow

Landed at sign-off in rinity `changes/pitch-rinity-1/`: PKT-02 (hold lifecycle 5-min backstop,
60 s poll, visit-type mapping, provider selection), PKT-03 (versioned config documents),
PKT-04 (blind transfer, carrier fallback, indefinite media), PKT-05 (grade/summary schema,
gpt-5.2 engine-side, token model), PKT-06 (attribution + correction history), PKT-07 (scrape
task model), PKT-08 (realm evidence as checklist rows), PKT-09 (Twilio path), PKT-10 (tap as
narrow engine addition, cap 3, audited), PKT-12 (run-label schema), PKT-13 (aggregates +
indefinite-media note), PKT-14 (per-phase evidence, dual-ack), PKT-15 (token TTL, retention,
revocation registry), PKT-16 (phone-collision rule, office-scoped persons), COVERAGE (EV-049
delta paragraph).

**Next station:** build via the workorder system (EV-048), each workorder citing its PKT(s) +
pitch tag; then session ④ working-software inspection, ⑤ RCDA cutover acceptance.
