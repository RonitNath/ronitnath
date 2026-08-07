# EV-049 — Data gate ③ rulings: the six escalations answered

- date: 2026-08-06
- source: owner, data gate session ③ review of `changes/pitch-rinity-1/data-model.md` §6 (ground truth, verbatim below)
- status: signed off

## Owner verbatim

> 1. hiqlite 2. use our openai credentials in secrets to call gpt-5.2. 3. sure 4. twilio yes, but more later 5. indefinite media, sure for rest 6. 5-min hold

## Consequences (numbered against the brief's §6)

1. **Control-plane store is hiqlite**, overriding the brief's SQLite-stays recommendation and
   matching the pilot's DEC-013 pattern. Single node now with the Raft/multi-node path open;
   DEC-013-style rules carry over: additive-only migrations for the bet, backup before the first
   product write, hiqlite's notify bus available for fan-out. → DEC-003.
2. **Grading and summaries call gpt-5.2 with the org's OpenAI credentials from the secrets
   store.** The execution home was not contradicted, so the brief's recommendation stands:
   engine-side, as post-call analysis extensions (AC-13), with the engine's provider registry
   configured with the org OpenAI key — rinity still holds no model credentials. *Interpretation
   flagged*: if the owner meant rinity-side execution, the seam moves; the gate record carries
   this reading explicitly so it can be corrected cheaply.
3. **Artifact posture accepted**: media stays only in the engine's private S3; rinity mints
   single-use short-TTL (15 min) revocable, office-scoped, audited download tokens. No projected
   copies.
4. **Twilio is the tier-1 productized carrier path — with more carriers later.** Additional
   carrier flows are named later work outside the bet, not a silent non-goal; Asterisk remains
   the synthetic-phase harness (DEC-001).
5. **Media retention is indefinite** — recordings and transcripts are retained indefinitely
   while the office is active, overriding the proposed 90-day default. The rest of §6.5 is
   accepted as proposed: metadata/grades/summaries/attributions/audit indefinite, test-call
   artifacts 30 days, artifact token TTL 15 min, listen-in media never stored, deletion-request
   path removes media + transcripts + summaries and keeps de-identified aggregates. The 1 h
   sudden-hangup transcript floor is trivially satisfied.
6. **Hold TTL backstop is 5 minutes** (not 15). Release-at-call-end semantics confirmed: any
   call end — normal or drop — releases all unresolved holds immediately; the 5-minute TTL only
   covers a lost call-end event. Exclusions and out-of-band PMS writes beat holds.

Unobjected adopted-by-default positions in §6's tail all bind at this sign-off: grade schema and
scales, run-label schema, phone-collision/confirmation rule, visit-type mapping, provider-
selection default, scrape task model, poll-first 60 s sync, blind transfer + carrier-level
outage fallback, ladder evidence artifacts + dual-ack advance authority, listener cap 3 with
listen audit, webhook default-off with rinity allowlisted.
