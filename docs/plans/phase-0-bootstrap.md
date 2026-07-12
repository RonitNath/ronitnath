# Phase 0 — Bootstrap

## Done by orchestrator
- GitHub: old `RonitNath/ronitnath` renamed `ronitnath-legacy`; fresh
  `RonitNath/ronitnath` created (public).
- Clone of stage_2 @ a126c64 with history; `origin` -> new repo,
  `upstream` -> stage_2. Baseline docs committed.

## Leg: two-bin split (port from events)
Reference: `~/dev/personal/events` — `src/bin/gather.rs`, `src/bin/admin.rs`,
`src/app.rs` (build_gather_router / build_admin_router / apply_layers),
Dockerfile, docker-compose.yml.

Scope (owned paths: whole repo; this leg runs alone):
- Split into `src/bin/site.rs` (public bin, default port 3130) and
  `src/bin/admin.rs` (auth'd bin, default port 3131), mirroring events'
  gather/admin split as closely as the current stage_2 skeleton allows.
  Site bin: public surface only (home, /healthz, /static, /api/client-errors)
  — no session middleware yet (phase 4 adds it deliberately).
  Admin bin: the full existing auth surface (login/signup/settings/account/
  guestbook exemplar) + attach_session, exactly as today.
- Port Dockerfile + docker-compose.yml from events, adjusted: image/service
  names `ronitnath-site`/`ronitnath-admin`, ports 3130/3131, same
  shared-`./data` volume pattern, AUTH_SIGNUP=closed on admin.
- Repo identity: package/binary naming, /healthz service name, README fork
  note, AGENTS.md merge-discipline section updated (upstream = stage_2, not
  stage_1). Mirror how events handled the same rename.

## Acceptance
- `cargo test` green (all existing stage_2 tests pass, split respected).
- Both bins boot locally (site on 3130, admin on 3131), `/healthz` OK on
  each; admin serves /login, site 404s it.
- `docker compose config` parses (build not required on mu).
- Evidence: commit hash, test output tail, curl/healthz output.
