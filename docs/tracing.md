# Targeted tracing pilot

`site` and `admin` can export OpenTelemetry traces directly to VictoriaTraces over OTLP/HTTP. Tracing is **off unless `TRACE_OTLP_ENDPOINT` is set**; that is the safe default for this fork and for a future web_template adoption.

On alien, VictoriaTraces 0.5.1 listens on `100.88.39.223:10428`. Its live OTLP/HTTP endpoint is:

```text
http://100.88.39.223:10428/insert/opentelemetry/v1/traces
```

The Grafana Jaeger datasource is already provisioned as VictoriaTraces at `http://127.0.0.1:10428/select/jaeger` on alien.

## Configuration

Put non-secret production values in each `[[binary]].env` block in `deploy/app.toml`. Both binaries use the same exporter settings, while their resources differ as `ronitnath-site` and `ronitnath-admin`.

| Variable | Default | Meaning |
| --- | --- | --- |
| `TRACE_OTLP_ENDPOINT` | unset | Enables tracing and names the OTLP/HTTP traces endpoint. |
| `TRACE_SLOW_THRESHOLD_MS` | `500` | Export requests strictly slower than this. |
| `TRACE_SAMPLE_RATE` | `0.0` | Deterministic baseline sample rate, clamped to `0.0..=1.0`. |
| `TRACE_FORCE_SECRET` | unset | Secret expected in `x-force-trace`; unset disables force tracing. |
| `TRACE_BUFFER_MAX_TRACES` | `1024` | Maximum simultaneously buffered trace decisions. |
| `TRACE_BUFFER_MAX_SPANS` | `32` | Maximum buffered spans per trace. |
| `TRACE_EXPORT_QUEUE` | `256` | Bounded queue of decided traces waiting for the exporter thread. |
| `TRACE_EXPORT_TIMEOUT_MS` | `1000` | Maximum OTLP export attempt duration, on the exporter thread only. |

`TRACE_FORCE_SECRET` comes from root-owned `/etc/ronitnath/tracing.env`, named by each binary's `environment_files` manifest entry. The file must be mode `0600` and never enter Git or `deploy/app.toml`. The secret is intentionally not logged or added to a span. A bad or absent header is treated exactly like an ordinary request.

## Sampling decision

Each request creates one root span named `HTTP {method} {matched_route}`. The route is Axum's matched template, never the raw URI, and all request-scoped logs inherit `request_id`, `trace_id`, and `span_id` fields. The root contains method, route, response status, duration, W3C ids, and a `trace.sample.reason` attribute when exported. Capability-bearing paths remain redacted in existing diagnostic logging.

At root-span end, the in-process processor keeps or drops the complete, bounded trace in this order:

```text
status >= 500                 export (server_error)
duration > slow threshold     export (slow)
valid x-force-trace secret    export (forced), return x-trace-id
deterministic rate sample     export (rate)
otherwise                     drop
```

The decision is deferred until the root closes, so ordinary requests at rate zero are never sent. Export happens on one dedicated worker through a bounded queue. A full buffer/queue drops telemetry; an unavailable backend is timed out and emits at most one warning per minute. Neither case waits on the HTTP request.

## Firewall dependency

Alien's declarative mesh firewall must include nexus in `traceProducers` using `mkTcpAllowRules 10428 traceProducers` in `/home/ronitnath/dev/alien/observability/obs-mesh-firewall.nix`. Do not append an ad-hoc iptables rule: the NixOS rule chain ends in a reject. Verify before deploying the app:

```sh
nc -vz -w 3 100.88.39.223 10428
```

## Forced trace walkthrough

With `TRACE_FORCE_SECRET` supplied by protected runtime configuration:

```sh
secret=$(sudo sed -n 's/^TRACE_FORCE_SECRET=//p' /etc/ronitnath/tracing.env)
curl -si -H "x-force-trace: $secret" https://ronitnath.com/healthz
```

Copy `x-trace-id` from the response, then query VictoriaTraces from alien:

```sh
trace_id=<response-header-value>
ssh alien "curl -fsS 'http://127.0.0.1:10428/select/jaeger/api/traces/$trace_id'"
```

In Grafana, choose the VictoriaTraces datasource and search that trace ID. The service emits both its JSON stdout record and a native journald record. The latter carries the correlation fields through `systemd-journal-upload`; journald uppercases custom field names, so find its exact lines in VictoriaLogs with:

```text
TRACE_ID:<response-header-value>
```

The operational validation script should also exercise a controlled 5xx and a request slower than `TRACE_SLOW_THRESHOLD_MS`, then repeat ordinary requests at rates `0.0` and `0.05`. Use a dead endpoint only in a temporary local or staged configuration to demonstrate that the bounded exporter cannot affect request latency.

## Lift into web_template

The reusable pieces are the subscriber setup, root-span/matched-route middleware, log correlation fields, and bounded deferred processor. This product fork's only app-specific decisions are its two binary names and deployment manifest. web_template would need a generic service-name input and an approved protected environment-file mechanism for `TRACE_FORCE_SECRET`; its default must remain disabled with `TRACE_SAMPLE_RATE=0.0`.
