# EV-048 — Round 9 accepted; build will run on the workorder system, not orchestration

- date: 2026-08-06
- source: owner, design gate session ② round 9 reaction (ground truth, verbatim below)
- status: accepted

## Owner verbatim

> lgtm. For the build, we'll use the workorder system instead of orchestration

## Consequences

1. **Round 9 accepted as-is**: availability v2 (per-provider weekly template + Time away list), the PRACTICE WEBSITE settings section, the F-8 onboarding boards (front door, streaming scrape, applied-vs-approval review, ask-only-gaps), and the capsule alignment on the F-4 breakpoint boards all stand. The acceptance cleanups are unblocked: the Availability sub-nav rewires to v2 and the old per-slot board is removed.
2. **Build execution model ruled**: when PITCH-001 reaches build (after data gate ③), the work runs through the workorder system (`context/procedures/coordination.md`) — the PM session authors workorder docs (`rinity/docs/workorders/YYYYMMDD-<slug>.md`: goal, scope+cwd, constraints, acceptance, out-of-scope), each indexed as a Forgejo issue in `isoastra/workorders` with exactly one lifecycle label and claim-before-execute; implementation flows issue → branch → PR → auto-merge-when-green (or direct-to-main where no CI). Workers are human-routed sessions. This **supersedes the orchestration default** (`context/procedures/orchestration.md` pi-delegation) for the rinity build: the PM/coordinator session does not dispatch pi legs for build work — it writes workorders and tracks them.
3. Work packets (PKT-01..PKT-16) remain the spec units; at build time each build increment is cut as a workorder that cites its packet(s) and the pitch tag, so delta discipline (changes/pitch-rinity-1/) carries through the workorder contracts.
