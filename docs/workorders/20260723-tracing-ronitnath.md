# Workorder: instrument + validate tracing on ronitnath.com

Date: 2026-07-23. Coordinator: mu session. Worker cwd: **nexus `~/dev/personal/ronitnath`**.

## Goal

Add OpenTelemetry trace export to the `site` binary (and `admin` if cheap), shipping to the existing VictoriaTraces backend on alien, with a **targeted/low-volume sampling shape** that is the candidate design for web_template. Prove end-to-end: request → span in VictoriaTraces → correlated journald log line in VictoriaLogs → visible in Grafana. This is a pilot: the sampling shape and its ergonomics are the deliverable as much as the wiring.

## Context (read, don't rediscover)

- App: two axum binaries (`site` public :3130, `admin` mesh :3131), sqlite/sqlx, tracing-subscriber structured logs already flowing journald → nexus `systemd-journal-upload` → alien VictoriaLogs. No /metrics endpoint; that stays out of scope.
- Traces backend: VictoriaTraces on alien (100.88.39.223) **:10428**, already provisioned in Grafana (alien :3000) as a Jaeger datasource at `/select/jaeger`. It ingests OTLP (`/insert/opentelemetry/v1/traces`); confirm exact path/port against the running version's docs before wiring.
- Producer instrumentation fleet-wide is deliberately tabled — this workorder is the sanctioned exception/pilot. Keep volume tiny; VictoriaTraces retention policy is unresolved.
- Alien mesh firewall is declarative: `obs-mesh-firewall.nix` in the alien flake checkout (`alien:/home/ronitnath/dev/alien`). mu is already allowed to :8480/:9428; **nexus → :10428 likely needs a new allow rule**. Gotcha (hard-won): use the declarative `mkTcpAllowRules` path — an ad-hoc `iptables -A` appends after the module's final reject and is silently dead. You have `ssh alien` + passwordless sudo from nexus. Verify with a direct TCP probe before and after.
- Deploy loop: push to main → `git pull && deploy/deploy.sh deploy` on nexus (webdeploy shim; socket units hold ports, zero refused connections). Repo rules: no remote branches — merge everything to main, push main only. Env changes go in `deploy/app.toml` `[[binary]]` env blocks (units are webdeploy-generated; never hand-edit units).

## Scope

1. **Wiring**: `tracing-opentelemetry` + `opentelemetry-otlp` (prefer OTLP/HTTP — no grpc dep) layered onto the existing `tracing-subscriber` stack. Resource attrs: `service.name` (`ronitnath-site` / `ronitnath-admin`), `service.version` = deployed git rev, `host.name`. Endpoint + all knobs via env, absent env ⇒ tracing layer fully disabled (web_template-safe default).
2. **Spans**: one root span per HTTP request named `HTTP {method} {matched_route}` — matched axum route template, never raw URI. Attributes: method, route, status, and W3C `trace_id`/`span_id`. Bounded cardinality: no query strings, no tokens, no person/guest identifiers. sqlx child spans are stretch — only if they fall out of existing instrumentation cheaply.
3. **Log correlation**: every request-scoped log line already emitted must carry `trace_id` (and keep the existing request-id if present) as a structured field, so LogsQL `trace_id:<id>` finds the exact lines.
4. **Sampling shape (the core deliverable)** — implement a per-request in-process "deferred tail" decision, no collector:
   - Buffer the request's spans; decide at root-span end.
   - **Always export** if: status ≥ 500, or root duration > `TRACE_SLOW_THRESHOLD_MS` (env, default 500), or the request carried `x-force-trace: <secret>` (secret from env; on forced requests, return `x-trace-id` in the response so the operator can jump straight to it).
   - Otherwise export at `TRACE_SAMPLE_RATE` (env, float; **default 0.0** — targeted-only steady state).
   - Non-exported buffers are dropped, not sent. If the exporter/endpoint is down: bounded queue, silent drop, zero effect on request latency — telemetry must never block a request.
   - If the OTel SDK's processor model makes true per-request buffering disproportionate, the documented fallback is: head-sample at `TRACE_SAMPLE_RATE` + a separate always-on cheap path that records error/slow requests. Prefer the buffered design; if you fall back, write down exactly why — that finding feeds the web_template decision.
5. **Docs**: `docs/tracing.md` in-repo — the env knobs, the sampling decision tree, the alien/firewall dependency, how to run a forced trace end-to-end, and a short "lift into web_template" section noting anything app-specific that would need generalizing.

## Acceptance (all must pass, evidence in the workorder-completion note)

- [ ] TCP probe nexus→alien:10428 succeeds; firewall change (if any) is committed in the alien flake, not ad-hoc.
- [ ] Deployed via the normal webdeploy loop; site + admin healthy; 0 non-200s from a light hammer during the deploy.
- [ ] Forced trace: request with `x-force-trace` returns `x-trace-id`; that exact trace is retrievable via Grafana Jaeger datasource (or `/select/jaeger` API) with correct route/status/duration.
- [ ] Error path: a seeded 5xx request appears as a trace with `TRACE_SAMPLE_RATE=0` set.
- [ ] Slow path: a seeded slow request (> threshold) appears likewise.
- [ ] Baseline silence: ~200 normal requests at rate 0.0 produce **zero** new traces.
- [ ] Sampled mode: with rate temporarily 0.05, ~200 normal requests produce roughly 5–20 traces (sanity band, not exact).
- [ ] Correlation: from one exported trace_id, a LogsQL query on alien VL returns that request's log lines.
- [ ] Resilience: with alien:10428 blocked (or endpoint env pointed at a dead port), a request hammer shows no latency regression and no unbounded memory; app logs a bounded/rate-limited export-failure note at most.
- [ ] Wrong-secret `x-force-trace` is ignored (no forced export, no trace_id header).
- [ ] `docs/tracing.md` written; changes merged to main, main pushed.

## Out of scope

- web_template changes (that's the follow-up once the shape is approved).
- Any OTel Collector deployment; metrics endpoints; dashboards beyond verifying the existing Jaeger datasource; VictoriaTraces retention policy; tracing for other apps/hosts; frontend/browser instrumentation.

## Status protocol

End substantial turns with one line: done/blocked/next. `[coord]`-prefixed messages are coordinator steering.
