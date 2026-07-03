# Plan: stage_1 hardening + HTTP test harness

Status: planned — to be implemented in stage_1 itself (not a fork).
Scope: everything generic and **auth-independent**. Anything that assumes an
authenticated actor (sessions, CSRF, roles, audit attribution) is explicitly
out of scope here — see `2026-07-secure-fork-template.md`, which builds on
this work in a downstream fork.

## Why

Evaluations of stage_1 forks keep flagging two classes of gap that belong to
the template, not the product: no HTTP-level tests (tests are store/query
only) and no baseline server hardening. Fixing both here means every fork —
isoastra_company, world_model, email-client-rcda, future forks — inherits
them silently, and evaluator agents stop reporting template gaps as product
gaps.

## 1. Middleware hardening (src/app.rs layer stack)

Add to the existing `ServiceBuilder` stack, keeping `app.rs` a readable
table of contents. Order matters; the stack after this change:

1. `SetRequestIdLayer` (existing — stays first)
2. `TimeoutLayer` — global request timeout, 30s default. Prevents a hung
   handler or slow-loris body from pinning a connection forever.
3. `RequestBodyLimitLayer` — 1 MiB default. stage_1 currently accepts
   unbounded bodies on `POST /api/guestbook` and `/api/client-errors`;
   both are unauthenticated writes.
4. Security response headers via `SetResponseHeaderLayer` (tower-http):
   - `X-Content-Type-Options: nosniff`
   - `X-Frame-Options: DENY`
   - `Referrer-Policy: strict-origin-when-cross-origin`
   - `Content-Security-Policy` — see CSP note below.
5. `TraceLayer`, error-page middleware, `PropagateRequestIdLayer`
   (existing — unchanged).

Constants for the timeout and body limit live in `config.rs` as new `Config`
fields with env overrides (`REQUEST_TIMEOUT_SECS`, `MAX_BODY_BYTES`),
following the existing "add tunables to Config, never `env::var` in
handlers" rule.

### CSP note

The FOUC-prevention block in `templates/_theme.html` is an inline script
(deliberately duplicated with `base.css` — see AGENTS.md). A strict
`script-src 'self'` breaks it. Implementation choice, in order of
preference:

1. **Hash the inline block**: `script-src 'self' 'sha256-<hash>'`. The block
   is static, so the hash is stable; compute it once and put it in a
   `const` next to the header definition with a comment pointing at
   `_theme.html`. A router test (below) asserts the hash still matches the
   template so drift fails CI, not production.
2. If (1) proves brittle in practice, fall back to
   `script-src 'self' 'unsafe-inline'` with a `TODO` — still better than no
   CSP because `default-src 'self'` blocks foreign fetch/img/frame targets.

Baseline policy:
`default-src 'self'; script-src 'self' 'sha256-…'; style-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'`

`frame-ancestors 'none'` makes `X-Frame-Options` redundant in modern
browsers; keep both anyway (belt-and-suspenders, old clients).

## 2. Rate limiting (unauthenticated write endpoints)

`/api/client-errors` and `/api/guestbook` are open POST endpoints — an
abuse/log-flooding vector on any public deployment. Add a small per-IP
token-bucket limiter:

- Prefer a tiny hand-rolled `tower` layer (in-memory `DashMap<IpAddr,
  bucket>`, ~60 lines, no new heavyweight dep) over `tower_governor` unless
  implementation finds a blocker — stage_1's philosophy is small, legible
  modules over frameworks.
- Applied per-route to the two POST endpoints, not globally (static assets
  and pages stay unthrottled).
- Defaults: 10 requests/min per IP, `429` with the standard error JSON shape
  from `error.rs`.
- Behind ingress, the client IP is `X-Forwarded-For` — resolve via
  `axum::extract::ConnectInfo` locally and a `TRUSTED_PROXY` config flag
  that switches to the forwarded header. Document in AGENTS.md that
  deployments behind wireguard/public ingress (see context repo
  `networks.md`) must set it.

## 3. Graceful shutdown

`run()` currently `axum::serve(...).await.unwrap()`s with no shutdown hook.
Add `with_graceful_shutdown` listening on SIGTERM + ctrl-c so in-flight
requests drain on deploy/restart. Log a clean "shutting down" line —
deployment tooling greps logs.

## 4. HTTP router test harness

The gap: `cargo test` exercises the store layer only; nothing asserts HTTP
behavior (status codes, headers, error pages, JSON shapes). Fix by testing
`build_router()` in-process with `tower::ServiceExt::oneshot` — no listener,
no port, stays inside the existing "<~1s, parallel-safe" budget.

Structure:

- `src/test_util.rs` (`#[cfg(test)]`-gated): `async fn test_app() -> Router`
  — builds `AppState` from `Store::connect_in_memory()` and returns
  `build_router(state)`. Plus small helpers: `get(app, path)`,
  `post_json(app, path, body)` returning `(StatusCode, HeaderMap, Bytes)`.
  Keep the crate a binary; in-crate `#[cfg(test)]` modules (matching the
  existing store-test style) rather than a `tests/` integration dir, which
  would require adding a lib target.
- `#[cfg(test)] mod tests` in `src/app.rs` with exemplar tests. These are
  **exemplars first, coverage second** — forks copy the pattern, so each
  test demonstrates one distinct assertion style:
  1. `GET /` → 200, `text/html`, body contains the layout marker.
  2. `GET /nonexistent` → 404 **and** the templated error page renders
     (exercises `render_error_pages` middleware) **and** the body echoes
     the `x-request-id` ("ref:").
  3. `POST /api/guestbook` valid JSON → 201/200, response parses to the
     generated type; then `GET /api/guestbook` shows the entry (full
     roundtrip through router + store).
  4. `POST /api/guestbook` malformed JSON → 4xx with the standard error
     JSON shape from `error.rs`.
  5. `GET /healthz` → 200, JSON has `version` / `uptime` fields.
  6. Oversized body → 413 (proves the body-limit layer is wired).
  7. Any response carries the security headers; the CSP `sha256-…` hash
     matches a fresh hash of the inline block in `_theme.html` (drift
     guard for the CSP note above).
  8. Rate limiter: 11th rapid POST from one client → 429.
- Every response also asserted to carry `x-request-id` (one helper, used
  everywhere).

## 5. Documentation updates (same change)

- AGENTS.md: add "Router tests" to the feature checklist (step: add a
  oneshot test per new route — copy the pattern in `app.rs`); document the
  new Config fields and the `TRUSTED_PROXY` deployment note.
- README.md: mention the hardening layers in the layout section
  (one line each — keep it a map, not a manual).

## Non-goals (deliberately excluded)

- Anything requiring identity: sessions, login, CSRF tokens, roles, audit
  attribution → secure-fork plan.
- HTTPS/TLS termination — handled by ingress per the deployment context.
- Playwright — AGENTS.md already defers it until a real multi-step flow
  exists.

## Verification

- `cargo test` — all new router tests pass, total suite stays under ~1s.
- `cargo run` + manual curl: headers present, 413 on big body, 429 on
  flood, SIGTERM drains cleanly.
- Screenshot `/` and `/guestbook` in both themes after CSP lands — the
  most likely CSP casualty is the theme script; the drift-guard test plus
  a visual check covers it (verification.md).

## Sequencing

Single PR-sized change, ordered: test harness first (tests 1–5 pass against
current behavior), then hardening layers (tests 6–8 added with them). That
way the harness validates the hardening rather than both landing untested.
