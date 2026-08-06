---
id: EV-010
date: 2026-08-06
provenance: inference
source: pi (gpt-5.5) read-only recon of isoastra/rinity @ 6bad375, nexus ~/dev/isoastra/rinity; work order + raw output in ~/tmp/rinity-extract on nexus; parent spot-check verified the RF-04 contract-pin divergence claim
---
The implemented feature surface of the rinity control plane, as built — the extraction pass
EV-007 calls for. Cite entries as `EV-010/RF-##`. Per EV-003 and EV-007 this is inventory,
not strategy: nothing here argues for its own survival (EV-009).

# Rinity implemented feature extraction

Read basis: README.md, docs/architecture.md, docs/design.md, docs/validation.md first, then source/config/deploy/tests inspection. This is an inventory of code reality, not a product recommendation.

## 1. Feature catalog

### RF-01 — Same-route signed-out door and signed-in console shell
- **What / for whom:** Anonymous visitors to `/` see an OIDC sign-in door; authenticated members at the same `/` route see a console header and hydrated console island. For practice/Isoastra operators using the browser console.
- **Entry points:** `GET /`; `console-root` island with `{ member }` prop; `/auth/oidc/primary/start` link from the door.
- **Key source paths:** `src/pages/home.rs`, `src/context.rs`, `src/app.rs`, `web/island-registry.ts`, `web/main.tsx`, `web/islands/ConsoleRoot.tsx`, `web/styles/app.css`.
- **Maturity:** `staged` — implemented; signed-out inert island has a component test, but I did not find route-level tests for the full anonymous/member document rendering.
- **Divergence:** Docs accurately say no `/app` silo and context-dependent root. Design says to show adapter freshness outcome; the console status text says "Live calendar updated" but freshness metadata from adapter responses is discarded before it reaches the island.

### RF-02 — Kanidm/OIDC browser-authenticated door with exact member admission
- **What / for whom:** Browser OIDC is composed from runtime env/config. Only identities whose provider subject key appears in `PRODUCT_MEMBER_SUBJECT_KEYS` are provisioned; provider claims do not become product roles directly. Session snapshot/CSRF material is exposed as JSON, not document markup.
- **Entry points:** `/auth/*` inherited OIDC routes; `GET /api/session`; all `/api/*` routes mounted under authenticated browser composition.
- **Key source paths:** `src/features/oidc.rs`, `src/features/session.rs`, `src/app.rs`, `config/providers.example.json`, `config/runtime.example`, `deploy/compose.yaml`.
- **Maturity:** `staged` — implementation is present through web_template runtime composition; local validation doc marks authenticated console walkthrough pending environment.
- **Divergence:** docs/validation.md says authenticated console is pending; that matches the absence of a local auth bypass. The exact-admission deployment requirement is implemented via `PRODUCT_MEMBER_SUBJECT_KEYS`.

### RF-03 — Multi-office tenancy data spine and scoped office listing
- **What / for whom:** Stores organizations, offices, lines, principals, organization memberships, and office memberships. Browser principals see offices through organization or office membership; office summaries omit adapter URL and credential reference. For Isoastra/practice operators.
- **Entry points:** `GET /api/console/offices`; repository API used by console handlers.
- **Key source paths:** `migrations/0001_rinity_core.sql`, `src/features/tenancy.rs`, `src/features/console.rs`, `tests/integration_adapters.rs`.
- **Maturity:** `working` for office creation/listing paths used by integration setup; `lines` and `office_memberships` are schema-only from a user-surface perspective.
- **Divergence:** docs accurately state organizations → offices → lines and role names. Code has no route/CLI to create lines, principals, or office memberships; only organization owner creation and office creation are exposed.

### RF-04 — Browser JSON admin creation of organizations and offices
- **What / for whom:** Authenticated browser session can create an organization, making itself owner, and then create offices only if it owns the organization. Offices carry timezone, adapter base URL, and adapter credential reference. For Isoastra operators/seeding workflows, not ordinary office staff UI.
- **Entry points:** `POST /api/admin/organizations`; `POST /api/admin/offices`.
- **Key source paths:** `src/features/console.rs`, `src/features/tenancy.rs`, `migrations/0001_rinity_core.sql`, `tests/integration_adapters.rs`.
- **Maturity:** `working` — integration test creates an org and office through these handlers.
- **Divergence:** There is no browser island/admin screen for these APIs. Docs say organization owners may create offices, which the code enforces; docs do not distinguish that this is JSON-only.

### RF-05 — PMS adapter credential-reference resolution
- **What / for whom:** Rinity stores only a credential reference on an office. At call time, the reference maps to `PMS_ADAPTER_CREDENTIAL_REF_<REF>` or `<REF>_FILE` env/runtime material; credential values are not returned in console APIs. For Isoastra operators and service runtime.
- **Entry points:** Adapter calls from `/api/console/*`; runtime env and secret files.
- **Key source paths:** `src/features/adapter.rs`, `src/features/tenancy.rs`, `src/features/console.rs`, `deploy/compose.yaml`, `docs/deploy.md`.
- **Maturity:** `working` for env/file resolver and integration-test resolver; production secret materialization is configuration, not tested here.
- **Divergence:** Docs match the credential-reference rule. `deploy/compose.yaml` still shows one demo credential ref env var wired to a file.

### RF-06 — PMS-adapter-contract v0.3 scheduling boundary
- **What / for whom:** All scheduling operations go through `pms-adapter-contract` HTTP `Client`; Rinity does not import or call a PMS/system of record directly. The client handles slots, appointments, patients, providers, book/update/cancel/confirm, and coverage hint.
- **Entry points:** `/api/console/slots`, `/appointments`, `/appointment`, `/patients`, `/providers`, `/book`, `/update`, `/cancel`, `/confirm`, `/coverage`.
- **Key source paths:** `Cargo.toml`, `Cargo.lock`, `src/features/adapter.rs`, `src/features/console.rs`.
- **Maturity:** `working` for slots/patients/book/update/get/providers against the native scheduling adapter integration test; `staged` for confirm and coverage (implemented, not UI-tested/integration-tested here).
- **Divergence:** README/docs claim v0.3 and code confirms package `pms-adapter-contract` version `0.3.0`. docs/validation.md claims a pinned rev `8cc8736...`; `Cargo.toml`/`Cargo.lock` are pinned to `3cfabd2027cf3ae6dc8b28631dc98faf321a5021`.

### RF-07 — Calendar availability and appointment browsing
- **What / for whom:** The console loads slots and persisted appointments for the selected office-local date range, supports month/week/day/provider views, navigation, date jump, today jump, loading/empty states, and short-lived per-office/range cache. For practice operators/office staff.
- **Entry points:** `GET /`; `ConsoleRoot` island; `GET /api/console/slots`; `GET /api/console/appointments`; `GET /api/console/providers`; `GET /api/console/patients`.
- **Key source paths:** `web/islands/ConsoleRoot.tsx`, `web/islands/calendar.ts`, `web/islands/ConsoleRoot.test.tsx`, `web/islands/calendar.test.ts`, `src/features/console.rs`, `src/features/adapter.rs`.
- **Maturity:** `working` — component tests cover persisted appointments/provider names, provider view, timezone/calendar math; adapter integration covers slot and appointment reads.
- **Divergence:** docs/architecture.md says cancelled appointments are loaded; server returns them, but the island filters cancelled appointments out of displayed/cache state. Design asks to show adapter freshness outcome; freshness is not surfaced to users.

### RF-08 — Adapter pagination, window splitting, and freshness merge
- **What / for whom:** Server-side adapter client follows cursors for slots, appointments, and patient search; splits long slot/appointment windows into <=31-day chunks, fetches chunks concurrently, deduplicates items, and merges freshness pessimistically. For operators requesting larger calendar ranges.
- **Entry points:** Same scheduling read APIs as RF-07.
- **Key source paths:** `src/features/adapter.rs`, `src/features/console.rs`, `tests/integration_adapters.rs`.
- **Maturity:** `working` for window splitting unit tests and native adapter read integration; large-pagination behavior is implemented but not specifically asserted beyond integration path.
- **Divergence:** docs/architecture.md accurately claims cursor following and concurrent chunks.

### RF-09 — Provider directory and weekly provider-load review
- **What / for whom:** Retrieves active provider labels from adapter `list_providers`, uses those labels for lanes/events, and provides a provider view with weekly scheduled percentage, visit counts, booked minutes, and opening counts. For practice operators/office staff.
- **Entry points:** Console provider view button; `GET /api/console/providers`.
- **Key source paths:** `web/islands/ConsoleRoot.tsx`, `web/islands/ConsoleRoot.test.tsx`, `src/features/console.rs`, `src/features/adapter.rs`, `tests/integration_adapters.rs`.
- **Maturity:** `working` — component test covers provider labels and weekly load; integration test verifies providers from native adapter.
- **Divergence:** docs/design.md says providers are derived from returned slots, but code uses `list_providers` for directory/labels and only uses slots for opening counts. docs/architecture.md matches the code.

### RF-10 — Patient search and appointment patient selection/correction
- **What / for whom:** Searches office-scoped patients through adapter `search_patients`, lets the user choose a patient before booking, and lets the user change the patient while editing an appointment. For office staff/practice operators.
- **Entry points:** Quick-create popover; edit appointment popover; `GET /api/console/patients`.
- **Key source paths:** `web/islands/ConsoleRoot.tsx`, `web/islands/ConsoleRoot.test.tsx`, `src/features/console.rs`, `src/features/adapter.rs`, `tests/integration_adapters.rs`.
- **Maturity:** `working` — component tests cover patient selection in booking and update body; integration test covers patient search.
- **Divergence:** Docs accurately say patient choices come from office-scoped `search_patients`. The island copy still references RCDA qualifier fixture data in quick-create, which is implementation/demo-specific rather than a general product surface.

### RF-11 — Test appointment booking
- **What / for whom:** Creates a test appointment from a selected slot and patient. The island performs an optimistic local insert only until the adapter result arrives, then refreshes live state; backend passes `client_op_id`/idempotency and a fixed note to adapter `book`.
- **Entry points:** Quick-create popover; keyboard Enter shortcut to first slot; `POST /api/console/book`.
- **Key source paths:** `web/islands/ConsoleRoot.tsx`, `web/islands/ConsoleRoot.test.tsx`, `src/features/console.rs`, `src/features/adapter.rs`, `tests/integration_adapters.rs`.
- **Maturity:** `working` — component tests cover booking error behavior and request body; integration test books against native adapter.
- **Divergence:** Docs call it disposable/test booking, which matches the fixed note/reason copy. Slot reads default to `recall` server-side when omitted.

### RF-12 — Appointment detail, move, resize, edit-in-place, and undo
- **What / for whom:** Users can open an appointment, edit date/time/duration/provider/patient, drag-move, resize top/bottom edges, and undo interval changes. Backend uses v0.3 explicit patient/provider/start/duration update; conflict/outage responses restore prior local state and refresh.
- **Entry points:** Calendar event click/pointer interactions; edit popover; `POST /api/console/update`; `GET /api/console/appointment` for persisted single appointment reads.
- **Key source paths:** `web/islands/ConsoleRoot.tsx`, `web/islands/calendar.ts`, `web/islands/ConsoleRoot.test.tsx`, `web/islands/calendar.test.ts`, `src/features/console.rs`, `src/features/adapter.rs`, `tests/integration_adapters.rs`.
- **Maturity:** `working` — component tests cover no accidental move on click, resize/undo, conflict vs outage messages; integration test updates and reads back an appointment.
- **Divergence:** Docs accurately describe explicit interval updates and `slot_taken`/`overlap` handling.

### RF-13 — Appointment cancellation, with limited UI undo
- **What / for whom:** Cancels an appointment via adapter and removes it from the calendar. UI offers an undo that tries to re-book the original slot if a slot id exists; otherwise it says the cancellation cannot be undone from the calendar. For office staff/practice operators.
- **Entry points:** Edit appointment popover; `POST /api/console/cancel`.
- **Key source paths:** `web/islands/ConsoleRoot.tsx`, `src/features/console.rs`, `src/features/adapter.rs`, `src/bin/rinity_mock_adapter.rs`.
- **Maturity:** `staged` — implemented, but I did not find component or native integration assertions for cancel in this repo; README claims broader cancel coverage than current test file shows.
- **Divergence:** README/docs/validation claim two-adapter integration drives cancel and verifies backend states; current `tests/integration_adapters.rs` does not call `/console/cancel`.

### RF-14 — Appointment confirmation API
- **What / for whom:** Authenticated API can confirm an appointment through adapter `confirm`. Intended for operator/staff workflows, but no UI currently calls it.
- **Entry points:** `POST /api/console/confirm` only.
- **Key source paths:** `src/features/console.rs`, `src/features/adapter.rs`, `src/telemetry_manifest.rs`.
- **Maturity:** `staged` — handler and adapter call exist; no island entry point or repo test found. The local mock adapter advertises `Confirm: false` and does not implement `/v0.3/confirm`.
- **Divergence:** docs/architecture.md includes confirmed appointments in loaded statuses and route matrix includes admin JSON generally; code has backend confirm but no UI.

### RF-15 — Coverage hint API with graceful unsupported response
- **What / for whom:** Authenticated API asks the adapter for a coverage hint by patient; if adapter returns `NotSupported`, Rinity maps it to `{ supported: false, hint: null }` rather than outage. For office staff/operator workflows that might later surface coverage.
- **Entry points:** `GET /api/console/coverage?office_id=...&patient_id=...` only.
- **Key source paths:** `src/features/console.rs`, `src/features/adapter.rs`, `src/telemetry_manifest.rs`.
- **Maturity:** `staged` — implemented; no island caller or repo test found.
- **Divergence:** docs/architecture.md claims this behavior, and code matches. README claims two-adapter integration includes graceful unsupported coverage; current integration test does not call coverage.

### RF-16 — Flat office configuration overlays
- **What / for whom:** Defines declared config keys with JSON schemas and per-office JSON overlay values. Rejects undeclared keys, empty keys, and floating-point JSON values during canonical serialization. For Isoastra operators/configuration automation.
- **Entry points:** Repository methods only (`ConfigRepository`); database tables. No HTTP route, CLI, or island found.
- **Key source paths:** `src/features/config.rs`, `migrations/0001_rinity_core.sql`, `src/app.rs`.
- **Maturity:** `dead` as a user-facing feature — model/repository exists and a float regression unit test exists, but no reachable product entry point uses it after app construction.
- **Divergence:** README/docs say Rinity adds config overlays; true at repository/schema level, overstated if read as an operator-usable product feature.

### RF-17 — Loopback-only mock PMS adapter fixture
- **What / for whom:** A disposable local adapter binary implements selected v0.3 routes with in-memory slots, patients, providers, appointment booking/update/cancel, idempotent operation replay, and bearer token `dev-mock`. It refuses non-loopback bind addresses. For developers/local adapter testing.
- **Entry points:** `cargo run --bin rinity_mock_adapter`; `/v0.3/*` routes on loopback.
- **Key source paths:** `src/bin/rinity_mock_adapter.rs`, `Cargo.toml`, `README.md`, `deploy/Dockerfile`.
- **Maturity:** `staged` — implementation exists, but production Dockerfile builds only `--bin rinity`; I did not find tests exercising this mock binary in the current repo.
- **Divergence:** README says production contains only Rinity and the mock is disposable; Dockerfile matches. Mock fixture does not implement confirm or coverage and advertises those verbs false.

### RF-18 — Public health and loopback operator runtime surface
- **What / for whom:** Runs public and operator listeners; public has `/livez`/`/healthz` via inherited runtime, operator has `/readyz` and `/metrics`. Telemetry manifest declares known HTTP routes and DB operation names. For Isoastra operators/deployment monitoring.
- **Entry points:** Public listener from `BIND_ADDR`; operator listener from `OPERATOR_BIND_ADDR`; `/livez`, `/healthz`, `/readyz`, `/metrics`.
- **Key source paths:** `src/main.rs`, `src/app.rs`, `src/telemetry_manifest.rs`, `src/database.rs`, `deploy/compose.yaml`, `docs/deploy.md`.
- **Maturity:** `staged` — composition/config exists; validation requires release evidence, but I did not run servers or tests.
- **Divergence:** docs/architecture.md route/auth matrix matches the intended listener split. Exact behavior of inherited system routers was not reimplemented in this repo.

### RF-19 — OCI deployment package shape
- **What / for whom:** Multi-stage Docker build compiles web assets and release `rinity` binary, copies only runtime binary/assets, uses digest-pinned base images, and Compose runs read-only host-networked service with state/secrets mounts and healthcheck. For Isoastra operators.
- **Entry points:** `deploy/Dockerfile`; `deploy/compose.yaml`; `docs/deploy.md`.
- **Key source paths:** `deploy/Dockerfile`, `deploy/compose.yaml`, `docs/deploy.md`, `package.json`, `vite.config.ts`, `src/main.rs`.
- **Maturity:** `staged` — deploy artifacts are present; validation.md says OCI/ingress/digest evidence is required, not present in repo.
- **Divergence:** README/docs say OCI/operator shape and no production mock sidecar; Dockerfile/Compose match. Ingress catalog registration is documented only, not represented by a checked-in catalog entry here.

## 2. Architecture spine

Rinity is an Axum service derived from web_template/runtime composition. `src/main.rs` builds one public listener and one operator listener; `src/app.rs` mounts `/` public document routes, `/auth` unguarded OIDC protocol routes, `/api` authenticated browser routes, and inherited public/operator system routers.

Tenancy is persisted in SQLite by one product migration: `organizations -> offices -> lines`, plus `principals`, organization memberships, office memberships, declared config keys, and office config overlays (`migrations/0001_rinity_core.sql`). Office access is not inferred from an organization singleton: `TenancyRepository::list_offices_for` returns offices where the browser-session subject has organization or office membership (`src/features/tenancy.rs`). Current exposed admin JSON creates organizations and offices only; line and membership management are not exposed.

Auth is a Kanidm/OIDC browser door through web_template crates (`browser-oidc`, `oidc-rp`, `secure-session`). `PRODUCT_MEMBER_SUBJECT_KEYS` is an exact allow-list; accepted external identities become hashed `oidc:<sha256>` subject ids (`src/features/oidc.rs`). Provider roles are not trusted as product roles.

Scheduling crosses the PMS boundary only through `pms-adapter-contract` version `0.3.0` (`Cargo.lock`) and `pms_adapter_contract::Client` (`src/features/adapter.rs`). Rinity stores adapter base URL and credential reference on offices; credentials resolve from env or runtime files immediately before adapter calls. There is no direct PMS dependency.

Config overlays are flat per-office JSON values gated by declared schema keys (`src/features/config.rs`). The system rejects floats for canonical JSON, but no product entry point currently exposes this repository.

Frontend is a server-rendered document with Solid islands (`islands`, `@isoastra/web-runtime`). The only island is `console-root`; Vite is pinned to `solidAppConfig({ scope: "dist" })`; CSS imports `@isoastra/tokens` in fonts -> scales -> style-brass -> style-ember -> base order (`web/styles/app.css`, `vite.config.ts`). Signed-out door uses brass/dark, signed-in console uses ember/dark (`src/pages/home.rs`).

Deploy shape is OCI-only: multi-stage Dockerfile builds web assets and Rust release binary, final image contains `/app/bin/rinity` plus `static/dist`; Compose uses read-only host networking, state and runtime-secret binds, durable SQLite, exact bind env, OIDC/provider files, and loopback operator healthcheck (`deploy/Dockerfile`, `deploy/compose.yaml`, `docs/deploy.md`). SQLx offline metadata is committed in `.sqlx/` (10 files). Migrations state is one product migration.

## 3. Notable absences

- **No voice engine wiring.** README says M1 does not deploy or wire a voice engine, and source search found no voice/call/SIP/Twilio/websocket/call-projection endpoints. `Cargo.toml` enables Axum `ws` and depends on `ws-log`, but this repo does not mount websocket or voice routes.
- **No stable engine attachment seam today.** The schema has `lines` and `agent_principal` role values, but there are no repositories/routes/CLI for line management, agent principal provisioning, agent auth, call event ingest, call state projections, or voice-to-scheduling command handoff. A future engine could reuse the PMS adapter client conceptually, but it cannot attach to a first-class Rinity engine API yet.
- **No eligibility/insurance workflow.** Coverage hint API exists backend-only, but there is no UI, persistence, or broader eligibility integration.
- **No patient lookup by id UI/API.** The adapter fixture marks lookup false; Rinity exposes search only.
- **No office/member/line management UI.** Organization/offices are JSON admin APIs; memberships and lines are schema-only.
- **No audit log, appointment history, or call interaction log.** `pms-adapter-contract` has a `LogInteraction` capability in the mock fixture enum map, but Rinity does not call it.
- **No deployment orchestration/ingress catalog entry in repo.** OCI artifacts exist; service-ingress registration is documented externally.
- **No product-owned scheduling persistence.** Appointments/slots/patients/providers are read from adapters and cached only in browser memory; Rinity does not persist them.

## 4. Recent trajectory

From `git log --format='%h %cI %s' -60` (only 28 commits exist) and file mtimes (all inspected repo files are timestamped 2026-08-01 16:00-18:36 in this checkout):

- 2026-07-31: repo derived from `product_template` v0.1.0, then M1 tenancy/config/adapter console landed.
- 2026-07-31: template example command vertical was removed; mock adapter was restricted to loopback.
- 2026-07-31: integration focus appeared (`test: prove console composition with two adapters`), but current test code only exercises the native scheduling adapter path.
- 2026-07-31: OCI handoff and exact member admission were prepared.
- 2026-07-31: deployed native scheduling adapter wiring was added/merged.
- 2026-07-31 to 2026-08-01: operator calendar views and PMS adapter calendar wiring were added.
- 2026-08-01: most work was calendar polish/defect closure: variable-duration calendar v2, drag snap behavior, quick-create rail containment, density/audit fixes, month paint indexing, persona defects.
- 2026-08-01: weekly provider load review and current design-token adoption landed.
- 2026-08-01: adapter compatibility/runtime-checked adapter query paths were removed, leaving the current v0.3 surface.

## 5. Confidence notes

- I did not run builds, tests, servers, browsers, or network calls per work order; maturity is based on source and test presence, not fresh execution.
- Inherited behavior from web_template/http-runtime/browser-oidc (exact CSRF mechanics, system health/router implementation) was treated as composed where mounted, not re-audited from dependency source.
- The README/validation two-adapter claims diverge from the current checked-in `tests/integration_adapters.rs`; it may reflect a removed or external test harness not present in this repo.
- File timestamps in this checkout are coarse/rebased-looking (mostly 2026-08-01), so trajectory relies primarily on git commit metadata.
