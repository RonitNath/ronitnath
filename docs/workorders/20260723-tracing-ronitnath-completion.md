# Tracing pilot completion

Completed 2026-07-23 on nexus. The deployed pilot uses OTLP/HTTP with an
in-process deferred-tail processor; no collector was added. The production
steady state is targeted-only (`TRACE_SAMPLE_RATE=0.0`).

## Deployment

- Main commits: `c116aaa`, `f93d6f6`, `51a931f`, and `6eb7afa` (native
  journald correlation fields).
- The normal webdeploy loop deployed the final commit to both `site` and
  `admin`; both services were active afterward.
- The final release smoke hammer reported `858` requests, `0` non-200s.
- The one-time webdeploy state-layout adoption exposed a legacy database-path
  mismatch and briefly prevented the site from opening SQLite. The manifest
  was corrected to the managed state path and the service was restored before
  the final deployment/hammers. The final zero-non-200 result applies to the
  final release transition, not that initial adoption attempt.

## End-to-end evidence

- Nexus can connect to alien `100.88.39.223:10428`; alien already had the
  declarative `mkTcpAllowRules 10428 traceProducers` rule including nexus, so
  no firewall change was required.
- A current forced `/healthz` trace, `9676c299135278e5d570cb660bc72018`, was
  retrievable through alien's Jaeger API. It had operation `HTTP GET
  /healthz`, status `200`, reason `forced`, `host.name=nexus`, and deployed
  service version `6eb7afa`.
- VictoriaLogs returned both native journal records for that trace using
  `TRACE_ID:9676c299135278e5d570cb660bc72018`, including `TRACE_ID`,
  `SPAN_ID`, `REQUEST_ID`, `HTTP_ROUTE`, and `STATUS`.
- With rate `0.0`, 201 observed normal response logs (the 200-request run plus
  one concurrent health check) yielded zero retrievable traces.
- With a temporary rate `0.05`, 201 observed normal response logs yielded 8
  retrievable traces. The protected runtime override was then removed and the
  deployed `0.0` value was re-confirmed.
- An exclusive SQLite lock seeded a slow `/e/{token}` response: `404` in
  1.794 s, trace `ea197165b2d7694cd8d4d66526a4cecd`, reason `slow`.
- Holding that lock beyond SQLite's busy timeout seeded a `500` in 5.010 s,
  trace `250ff432481679137fc555bf7f3b6be1`, reason `server_error`.
- A wrong `x-force-trace` secret returned no `x-trace-id` header.
- With a temporary dead exporter endpoint, 100 forced requests remained
  `200` (2.124 s total versus 1.734 s with the live endpoint). The bounded
  worker emitted one rate-limited failure warning; its native and JSON journal
  representations account for two visible journal entries. The live endpoint
  and rate-zero production configuration were restored afterward.

## Validation

`cargo fmt --check`, `cargo test telemetry::tests -q`, and
`cargo check --all-targets -q` passed before the final deploy. The final
runtime checks confirmed both socket-activated services active and the
current release at `6eb7afa`.
