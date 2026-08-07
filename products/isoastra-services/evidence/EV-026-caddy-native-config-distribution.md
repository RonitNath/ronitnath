---
id: EV-026
date: 2026-08-06
provenance: inference
source: agent research at owner's prompt ("caddy has a pretty solid plugin system already right? see if there's something native") — Caddy v2 source at master, verified against the stock `caddy v2.11.2` binary running on nanode
---
Caddy ships a **native pull-based config distribution mechanism** that fits this bet's applicator
question (OQ-1) without a plugin, an xcaddy rebuild, or any Go.

## `caddy.config_loaders.http` — the pull loader

Present in the **stock** binary on nanode today (confirmed in `caddy list-modules`, v2.11.2). Set
`admin.config.load` to it and the instance fetches its entire config from a URL.

Fields (`caddyconfig/httploader.go`): `url`, `method` (default GET), `header`, `timeout`, `adapter`,
and `tls` — `use_server_identity` (present Caddy's own managed identity cert), or
`client_certificate_file` + `client_certificate_key_file`, plus `root_ca_pem_files`. So the edge
authenticates to the control plane over **mTLS natively**. Built-in retry: 10 attempts, 500ms apart.

The `adapter` field matters for this design: the response need not be JSON. The control plane can
serve **Caddyfile text** and the edge adapts it — so the service keeps rendering the same artifact
the current daemon renders, and the wire format stays human-reviewable.

Applying a loaded config goes through the same path as the admin API's `POST /load`, so it is a
whole-config atomic swap with Caddy's ordinary graceful reload semantics.

## The trap, and it is easy to miss

`load_delay` polls, but **the polling loop exits as soon as a changed config is successfully
applied**. From `caddy.go`: *"the loop is here to iterate ONLY if there is an error, a no-op config
load, or an unchanged config; in which case we simply wait the delay and try again"* — on success it
`break`s. Polling continues only because the *newly loaded config* carries its own
`admin.config.load` block. Caddy supports this recursion deliberately and guards the footgun:
`"recursive config loading detected: pulled configs cannot pull other configs without positive
load_delay"`.

So: **every config this service serves must include the loader block with a positive `load_delay`,
or the edge pulls once and goes deaf forever.** The field was renamed `LoadInterval` → `LoadDelay`
upstream precisely because it reads as an interval and is not one.

## Why this fits the bet

- **Failure semantics are what EV-016 asks for.** Control plane unreachable → `LoadConfig` errors →
  Caddy logs and retries → **the running config keeps running**. The data plane does not depend on
  the control plane being up. On reboot, Caddy's autosaved config plus `--resume` brings the edge
  back on its last-known-good routes with the control plane still down.
- **Credential direction inverts.** The cluster holds no SSH key and no write credential to any
  public node. Each edge holds a client cert that only lets it *read* its own config. Compromising
  the control plane cannot mutate an edge except by serving it config; compromising an edge yields
  read access to that edge's routes.
- **Nothing new to install.** The applicator is the Caddy already running (EV-022's "natural" bar).
- **No Go.** The service's routing surface reduces to a read-only HTTP endpoint that renders an
  edge's complete Caddyfile — which suits the Rust stack (EV-002) and is a very small surface.

Costs, honestly: whole-config replacement only (no partial route patch — acceptable, the model is
desired-state anyway); change latency equals the poll delay rather than being immediate; and the
recursion trap above is a permanent property the config renderer must never violate.

## The push alternative, also native

`admin.remote` (`RemoteAdmin`) starts a second admin listener (default `:2021`) enforcing **mutual
TLS**, with per-identity access control down to allowed API paths and HTTP methods
(`AdminAccess.Permissions`). It is the real push mechanism and it is well-secured — but the source
marks it **"EXPERIMENTAL: Subject to change"**, and it puts mutate-everything credentials in the
control plane, which is the property the pull loader avoids.

## Certificates across many edges — an unraised problem

N edges serving the same hostnames means N Caddys each running ACME for them. CertMagic's model is
that **shared storage is what makes instances a cluster**: instances on the same storage coordinate
so only one performs the ACME exchange and the others read the result. Stock Caddy ships only
`caddy.storage.file_system`; Redis/Consul/S3/Postgres storage backends are third-party modules
requiring an xcaddy rebuild. Let's Encrypt's limit is 50 certificates per registered domain per
week, so duplicate issuance across edges is a real ceiling, not a theoretical one.

Two native ways out, both stock on nanode: `tls.get_certificate.http` (on handshake the edge fetches
the cert chain + key as PEM from a URL, with SNI passed as `server_name`; HTTP 204 means "not mine")
and `tls.permission.http` (on-demand issuance gated by an ask endpoint that returns 200/not-200 for
`?domain=`). Caddy also ships its own CA (`pki`, `tls.issuance.internal`, `http.handlers.acme_server`)
— it can *be* the ACME server for internal names.

This is a design-gate question this bet must answer, not a T1 scope expansion: multi-edge (EV-013)
is not achieved by route distribution alone if every edge fights over the same certificates.

## Also native, possibly relevant

`http.reverse_proxy.upstreams.a`, `.srv`, `.multi` — dynamic upstreams resolved from DNS at request
time rather than baked into config, which would let upstream membership change with no config push
at all.
