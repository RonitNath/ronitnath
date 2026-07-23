# Workorder: ronitnath.com wave — strip, Solid retrofit, Rauthy guest login, agenda calendar, tracing

Date: 2026-07-23. Coordinator: mu session. Worker cwd: **nexus `~/dev/personal/ronitnath`**.

One wave, five legs, ordered. Legs A→D are sequential; leg E (tracing) is backend-only and may run at any point, including first. Deploy after each leg via the normal webdeploy loop — small verified increments over one big-bang flip.

## Standing constraints (all legs)

- Live site. Every deploy goes through `deploy/deploy.sh deploy` (webdeploy; socket units hold ports). Zero refused connections is the norm — hammer lightly across each deploy.
- Repo rules: no remote branches — merge to main, push main only. Sqlx migration checksums are LF-sensitive (checksum repair pattern is in repo history if hit).
- Env/knob changes go in `deploy/app.toml` `[[binary]]` env blocks; units are webdeploy-generated, never hand-edited. Keep `ListenStream` and `BIND_ADDR`/`ADMIN_BIND_ADDR` in sync if touched.
- Visual work is bound by the repo design brief `docs/design.md` and context `procedures/{design,verification}.md` (context checkout on nexus: `~/dev/context`). Every user-visible change gets browser-level verification (agent-browser is available on nexus) — screenshots or persona walk evidence, not just curl.
- No destructive data migrations in this wave. Removing a feature removes routes/UI, not tables or rows.

## Leg A — strip to intentional features

Remove `/about` and `/guestbook` (routes in `src/app.rs:117-118`, handlers, templates, the `guestbook.ts` island entry, and all nav links to them). Intentional surface that remains: home `/`, `/events`, `/e/{token}` invites + claim, guest login + `/my`, photos, `/calendar` + ICS feeds, settings, admin.

- Removed routes return 404 (no redirects, no tombstone pages). Nav/footer/sitemaps show no dangling references.
- Guestbook tables and rows stay in the DB untouched; note their names in the completion report so a future cleanup migration is deliberate.

## Leg B — Solid islands retrofit

Replace the vanilla-DOM `ts/` islands with Solid islands per current `RonitNath/web_template` conventions (pnpm + vite + Solid — read the current foundation's islands setup and AGENTS.md before starting; the foundation, not this repo, is the pattern authority). In scope: `event_rsvp`, `events_admin`, and the `site.ts` chrome (menu, theme toggle, error reporting). `guestbook.ts` died in leg A.

- Adopt the pnpm workspace/pipeline as web_template does it (nexus has pnpm via corepack shim). The no-package-manager esbuild pattern is retired for this repo.
- Server-rendered pages stay server-rendered — this is an islands swap, not an SPA-ification. Mount points and progressive-enhancement behavior (pages usable with JS off where they are today) are preserved.
- Acceptance: RSVP flow and admin event editing behave identically pre/post (walk both in a browser); `?v=GIT_HASH` asset busting still works; deploy loop still builds islands inside the devshell without new global tooling beyond pnpm/node.

## Leg C — Rauthy guest login + `/my` account management

Goal: friends log in via the personal-universe IdP (Rauthy at `id.ronitnath.com` — NOT Kanidm; three-universe identity model) and manage their own event history and registrations.

The RP machinery already exists — read `docs/oidc-factor.md` first. Guest sign-in policy (claim-gated provisioning via `person_identity_links`, fail-closed bare logins) is already implemented and must not be loosened. The work is:

1. **Provider wiring**: register a confidential OIDC client for ronitnath.com in Rauthy (redirect URI `https://ronitnath.com/auth/oidc/rauthy/callback`; PKCE S256 is always used by the RP). For Rauthy admin access and client-lifecycle procedure read context `procedures/idp.md` and the personal-network entity context; secrets handling per `resources/secrets.md`. Drop the provider entry in the `OIDC_PROVIDERS_PATH` JSON (production path lives with app state, not in the repo; secret never committed). `AUTH_SIGNUP` stays closed; the provider's trust grant is bounded by the existing guest-claim policy.
2. **`/my` becomes real account management**: authenticated guests see their event history (past events they were linked to, with what's visible at their audience level) and upcoming registrations, can change their RSVP for open events (same rules as the invite-link RSVP island — no new write powers), and can mint/rotate their personal calendar feed (existing `mint-calendar-feed` capability, self-service). Settings page shows the linked Rauthy identity via the existing link/unlink factor flow with its last-factor guard.
3. **Claim → login lifecycle**: end-to-end proof with a fresh test person — mint invite link, claim via Rauthy as a brand-new `{issuer}#{sub}`, land on `/my`, change an RSVP, log out, log back in via bare guest login (now allowed, since the person link exists), verify history renders.

- Negative acceptance (must fail closed): bare Rauthy login with an unlinked identity; claim on a revoked link; claim on an already-claimed person; second identity attempting to link an already-linked person.
- New `/my` UI is built Solid-native (post leg B), to the design brief.

## Leg D — agenda-first calendar

`/calendar` defaults to an agenda view (chronological upcoming list, grouped by day/week, past events accessible but not the landing state); any grid/month view becomes secondary if kept at all. Applies to the public page and the logged-in guest's view; ICS feeds unchanged. Design-brief styling + browser verification like the rest.

## Leg E — tracing (separate contract, same wave)

Execute `docs/workorders/20260723-tracing-ronitnath.md` as written — its scope and acceptance matrix are the contract for this leg (OTLP export to alien VictoriaTraces :10428, alien firewall allow via the declarative path, deferred-tail sampling with rate 0.0 default / error / slow / `x-force-trace`, log correlation, resilience proofs). Backend-only; may run before, between, or after legs A–D.

## Wave acceptance

- [ ] All leg-level acceptance above green, with browser-walk evidence for A–D and the tracing matrix for E.
- [ ] Full persona pass at the end: anonymous visitor (home → events → calendar), invited friend (claim → RSVP → /my → re-login), owner admin (event edit, calendar entry, link mint) — no dead links, no design-brief violations, 0 non-200s during the final deploy.
- [ ] `docs/` updated where behavior changed (oidc-factor.md provider example, deploy.md if pipeline changed, tracing.md from leg E). Architecture contract `docs/plans/000-architecture.md` amended for the removed surfaces.
- [ ] Everything merged to main, main pushed, deployed, live-verified at ronitnath.com.

## Out of scope

Session replay (separate follow-up); web_template merge-back of any of this; guestbook/about data deletion; Kanidm or isoastra-universe identity changes; Rauthy server configuration beyond registering this one client; telegram integration.

## Status protocol

End substantial turns with one line: done/blocked/next. `[coord]`-prefixed messages are coordinator steering. This workorder supersedes the standalone sequencing note in the tracing workorder (that file remains the leg-E contract).
