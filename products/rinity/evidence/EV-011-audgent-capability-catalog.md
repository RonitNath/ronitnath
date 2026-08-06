---
id: EV-011
date: 2026-08-06
provenance: inference
source: pi (gpt-5.5) read-only recon of isoastra/audgent @ 312ece2, nexus ~/dev/isoastra/audgent; work order + raw output in ~/tmp/rinity-extract on nexus
---
The capability surface of the audgent engine, as built — the engine half of the EV-007
extraction, in scope per EV-008. Cite entries as `EV-011/AC-##`. Inventory, not strategy
(EV-003); the integration-surface section is the seam rinity's design work will draw against.

# Audgent capability extraction

Read-only source reconnaissance of `/home/ronitnath/dev/isoastra/audgent` on 2026-08-06. I did not run builds, tests, servers, models, or network calls.

## 1. Capability catalog

### AC-01 — Telephony ingress and egress

- **What exists:** Inbound and outbound voice calls through Twilio, Vonage, Plivo, Telnyx, Cloudonix, Vobiz, and Asterisk ARI. The generic inbound path is `POST /api/v1/telephony/inbound/run`, which detects provider, matches provider account + called number to an org telephony config, verifies provider signature, creates a workflow run, and returns provider-specific stream instructions. Outbound calls use `POST /api/v1/telephony/initiate-call` for console/test calls and `POST /api/v1/public/agent/...` for service-principal API triggers.
- **Protocols:** Provider webhooks plus provider media WebSockets; ARI uses `chan_websocket` at `/api/v1/telephony/ws/ari`; generic media path is `/api/v1/telephony/ws/{workflow_id}/{organization_id}/{workflow_run_id}`.
- **Lifecycle handling:** Run creation, initialized→running transition, provider metadata in `workflow_runs.gathered_context`, per-org concurrency slot acquisition/release, hangup/fallback responses, transfer result callbacks, quota checks.
- **Key source paths:** `api/routes/telephony.py`; `api/services/telephony/registry.py`; `api/services/telephony/factory.py`; `api/services/telephony/providers/*`; `api/services/telephony/base.py`; `docs/integrations/telephony/*.mdx`.
- **Maturity:** Working in source for the listed providers; provider depth varies. `docs/integrations/telephony/agent-stream.mdx` says Agent Stream is Cloudonix-only. The telephony WebSocket itself has an in-code TODO noting that the URL triple is currently a bearer capability without a one-shot token.
- **Provenance:** Mostly upstream-inherited provider architecture. Post-`894b0e2` Isoastra changes touched dispatch/auth seams and removed WebRTC/embed paths, but provider folders themselves show no post-marker commits in `git log 894b0e2..HEAD -- api/services/telephony/providers/*`.

### AC-02 — Browser/operator-console voice calls over raw PCM WebSockets

- **What exists:** Browser test calls no longer use WebRTC. The console opens `/api/v1/ws/events/{workflow_id}/{workflow_run_id}` for JSON control events and `/api/v1/ws/media/{workflow_id}/{workflow_run_id}` for raw PCM16 mono audio. The media socket announces sample rate with a `ready` JSON message, then expects a `start` message with call context and binary PCM thereafter.
- **Key source paths:** `api/routes/console_media.py`; `api/routes/call_events.py`; `api/services/pipecat/transport_setup.py`; `api/services/pipecat/browser_audio_serializer.py`; `harness/README.md`; `CHANGELOG.md`.
- **Maturity:** Working/staged; this is the path used by the T9 harness and capacity harness.
- **Provenance:** Isoastra fork change after upstream marker (`ce51597`, `2671c96`): WebRTC media plane and widget removed, console media moved to same-host WebSockets.

### AC-03 — Agent Stream WebSocket

- **What exists:** `wss://.../api/v1/agent-stream/{provider}/{workflow_uuid}` creates an inbound run from a workflow UUID and hands the WebSocket to the provider's `handle_external_websocket`.
- **Key source paths:** `api/routes/agent_stream.py`; `docs/integrations/telephony/agent-stream.mdx`; Cloudonix provider code under `api/services/telephony/providers/cloudonix/`.
- **Maturity:** Staged/limited. Documentation explicitly says Cloudonix only; other providers raise `NotImplementedError` until implemented.
- **Provenance:** Upstream-inherited with Isoastra auth/model-proxy deletion edits.

### AC-04 — Non-realtime voice pipeline: STT → LLM → TTS

- **What exists:** Pipecat pipeline with transport input, STT, user aggregator, LLM, engine callback processor, optional recording router, TTS, interruption epoch guard, transport output, audio recording buffer, assistant aggregator, and metrics aggregator.
- **STT providers in typed config:** Deepgram, Cartesia, OpenAI, Google, Sarvam, Speechmatics, Speaches, Hugging Face, AssemblyAI, Gladia, Azure Speech, ElevenLabs realtime STT, Smallest AI.
- **LLM providers in typed config:** OpenAI, Google, Google Vertex, Groq, OpenRouter, Azure OpenAI, AWS Bedrock, Speaches/OpenAI-compatible, Hugging Face, MiniMax, Sarvam.
- **TTS providers in typed config:** Deepgram, ElevenLabs, Google, OpenAI, Cartesia, Inworld, Sarvam, Camb.ai, Rime, Speaches, MiniMax, Azure Speech, Smallest AI, xAI.
- **Key source paths:** `api/services/pipecat/run_pipeline.py`; `api/services/pipecat/pipeline_builder.py`; `api/services/pipecat/service_factory.py`; `api/services/configuration/registry.py`; `api/schemas/ai_model_configuration.py`.
- **Maturity:** Working by source and test coverage names; not live-verified in this recon.
- **Provenance:** Upstream-inherited provider breadth, with Isoastra changes around MPS removal, BYOK-only org config, URL security, and performance fixes.

### AC-05 — MiniCPM5/local inference selection

- **What exists:** MiniCPM5 is not hardcoded as a special provider. It is selected through the Speaches/OpenAI-compatible LLM path: `provider="speaches"`, `model="jewelzufo/MiniCPM5-1B"`, `base_url` pointing at an Ollama OpenAI-compatible endpoint. `SPEACHES_LLM_EXTRA_BODY` is merged into Speaches chat requests; documented local loop sets `{"reasoning_effort":"none"}` to suppress MiniCPM5 reasoning.
- **Key source paths:** `api/engine_settings.py` (`speaches_llm_extra_body`, `default_model_configuration_json`); `api/services/pipecat/service_factory.py` (`SpeachesLLMSettings(... extra=SPEACHES_LLM_EXTRA_BODY)`); `api/services/configuration/registry.py` (`SpeachesLLMConfiguration`); `docs/workorders/20260721-local-loop-bringup.md`; `docs/workorders/20260721-think-filter.md`; `bench/README.md`; `bench/voicebench.py`.
- **Maturity:** Staged/local-loop. Source supports it and workorders/bench document it, but no committed deployment default in the repo pins MiniCPM5; actual selection is env/DB configuration.
- **Provenance:** Isoastra fork change/local-stack work.

### AC-06 — Realtime speech-to-speech mode

- **What exists:** Alternate pipeline where realtime services handle STT+LLM+TTS internally. Supported typed providers: OpenAI Realtime, Grok Realtime, Ultravox, Google Gemini Live, Google Vertex Gemini Live, Azure Realtime. A conventional LLM is still required for extraction/QA side-channel work.
- **Key source paths:** `api/services/pipecat/run_pipeline.py`; `api/services/pipecat/pipeline_builder.py`; `api/services/pipecat/service_factory.py`; `api/services/pipecat/realtime/*.py`; `api/services/configuration/registry.py`; `docs/configurations/inference-providers.mdx`.
- **Maturity:** Working/staged by source and tests; not live-verified here.
- **Provenance:** Upstream-inherited plus Isoastra/Pipecat-fork compatibility changes.

### AC-07 — VAD, turn-taking, interruptions, idle/max-duration handling

- **What exists:** Shared Silero VAD analyzer; configurable turn start strategies (`min_words`, provisional VAD, transcription/VAD, external provider turns); turn stop via speech timeout, smart turn analyzer, or provider external EOT. Per-node `allow_interrupt` gates barge-in; user mute strategies mute until first bot completion, during function calls, queued transition/tool speech, or non-interruptible bot speech. Max call duration and user idle timeout are workflow config fields.
- **Key source paths:** `api/services/pipecat/run_pipeline.py`; `api/services/pipecat/shared_silero_vad.py`; `api/services/workflow/pipecat_engine.py`; `api/services/pipecat/interruption_epoch_guard.py`; `api/tests/test_shared_silero_vad.py`; `api/tests/test_interruption_epoch_guard.py`; `docs/workorders/20260801-62-alien-capacity.md`.
- **Maturity:** Working; capacity work specifically optimized shared Silero VAD and on-demand recording loading.
- **Provenance:** Mixed: upstream Pipecat capabilities plus Isoastra fork tuning and pipecat submodule changes around turns/interruption.

### AC-08 — Conversation/agent logic as workflow graph

- **What exists:** Agents are workflows: a ReactFlow-style directed graph with `startCall`, `agentNode`, `endCall`, `globalNode`, `trigger`, `webhook`, and `qa` nodes. Prompts support template variables; edges become LLM function-call transition tools with natural-language conditions, optional transition speech/audio, and guard evaluation. Workflow definitions are versioned; runs pin a definition.
- **Key source paths:** `api/services/workflow/dto.py`; `api/services/workflow/workflow_graph.py`; `api/services/workflow/pipecat_engine.py`; `api/routes/workflow.py`; `api/routes/node_types.py`; `docs/core-concepts/workflows-and-agents.mdx`; `docs/voice-agent/*.mdx`.
- **Maturity:** Working; this is the core runtime.
- **Provenance:** Upstream-inherited core with Isoastra additions to immutable contract/eval fixtures and service-principal caller seam.

### AC-09 — Tool calling, knowledge, and PMS-adjacent actions

- **What exists:** Reusable tools can be HTTP API tools, end-call tools, transfer-call tools, calculator tools, and MCP tools. Tools attach per node and are exposed as LLM function schemas. HTTP tools can include model-supplied parameters, preset/template parameters, stored credentials, and custom headers. Transfer supports static/template destinations, dynamic HTTP resolver, or context mapping; docs say transfer works for Twilio, Telnyx, and Asterisk ARI. Knowledge-base retrieval is a built-in function when document UUIDs are attached. Pre-call fetch can POST to an external system before the agent speaks.
- **PMS-adjacent evidence:** The repo contains Rinity test-fixture endpoints for appointment slots, booking, reschedule, eligibility, and messages under `/api/v1/test-fixtures/rinity/*`. These are fixtures, not a real PMS integration. Generic HTTP tools and dynamic transfer resolver are the actual integration seams.
- **Key source paths:** `api/schemas/tool.py`; `api/routes/tool.py`; `api/services/workflow/tools/custom_tool.py`; `api/services/workflow/tools/transfer_resolver.py`; `api/services/workflow/tools/knowledge_base.py`; `api/services/workflow/pipecat_engine.py`; `api/routes/rinity_test_fixtures.py`; `docs/voice-agent/tools/*.mdx`; `docs/voice-agent/pre-call-data-fetch.mdx`.
- **Maturity:** HTTP/MCP/end-call/transfer/KB are working by source; Rinity PMS endpoints are stub/test fixtures.
- **Provenance:** Mostly upstream-inherited tool system; Rinity fixtures and some transfer/context mapping changes are Isoastra-specific.

### AC-10 — Outbound campaigns and API-triggered calls

- **What exists:** Public service-principal API can start calls by trigger path or workflow UUID against published or draft definitions. Campaigns store CSV/source-backed queued runs, rate limits, retry policy, and circuit breaker state. Console can create a run for WebSocket test calls.
- **Key source paths:** `api/routes/public_agent.py`; `api/routes/workflow.py`; `api/routes/campaign.py`; `api/db/models.py` (`CampaignModel`, `QueuedRunModel`, `AgentTriggerModel`); `api/services/campaign/*`; `docs/voice-agent/api-trigger.mdx`.
- **Maturity:** Working by source; campaigns not deeply verified in this recon.
- **Provenance:** Upstream-inherited, with Isoastra service-principal attribution/auth changes.

### AC-11 — Multi-tenancy, identity, and per-tenant configuration

- **What exists:** Organizations are tenants. Users have memberships/roles in `organization_users`; workflows, telephony configs, phone numbers, credentials, tools, model configuration, preferences, service principals, and runs are org-scoped. Production auth provider is Kanidm; local email/password is refused outside local/test. Machine callers authenticate as service principals via OAuth2 client credentials + token introspection + local `service_principals` row with scopes.
- **Per-tenant behavior:** Org-level model config v2, telephony configurations and phone-number inbound workflow assignment, external credentials, tools, Langfuse credentials, preferences, default concurrency. Workflows can override model config selectively.
- **Key source paths:** `api/db/models.py`; `api/services/auth/service_principal.py`; `api/services/auth/scopes.py`; `api/routes/organization.py`; `api/services/configuration/ai_model_configuration.py`; `api/engine_settings.py`.
- **Maturity:** Working; post-fork identity hardening is central.
- **Provenance:** Heavily Isoastra fork change after upstream marker (`P1-*`, service principal, Kanidm, tenant-role commits).

### AC-12 — Observability: transcripts, recordings, traces, metrics, logs

- **What exists:** Realtime feedback events stream to the console and are persisted in `workflow_runs.logs.realtime_feedback_events`. Mixed, user, and bot WAV buffers plus transcript text are uploaded to S3/MinIO; runs expose public download URLs via per-run access tokens. Pipeline metrics aggregate LLM/TTS/STT usage and call duration into `workflow_runs.usage_info`. OTEL tracing can export to Langfuse or plain OTLP, including org-specific Langfuse routing. TTFB/latency events are streamed. Platform admin exposes workload/run views.
- **Key source paths:** `api/services/pipecat/event_handlers.py`; `api/services/pipecat/pipeline_metrics_aggregator.py`; `api/services/pipecat/tracing_config.py`; `api/services/workflow_run_artifacts.py`; `api/routes/workflow.py`; `api/routes/public_download.py`; `api/routes/platform_admin.py`; `api/routes/reports.py`; `docs/developer/webhooks.mdx`.
- **Maturity:** Working by source; live trace export not verified. Langfuse disabled unless configured.
- **Provenance:** Upstream-inherited observability plus Isoastra changes removing third-party telemetry and adding OTLP/Victoria-friendly paths and platform workload analysis.

### AC-13 — QA, durable final webhooks, and structured post-call analysis

- **What exists:** `qa` nodes run post-call LLM analysis over node-split transcripts with sampling/min-duration/voicemail filters, storing results in `workflow_runs.annotations`. Webhook nodes enqueue durable `webhook_deliveries` with bounded retry/dead-letter handling; payloads include context, annotations, artifacts, and disposition. A separate structured post-call analysis produces `workflow_run_analyses` rows with summary/outcome/intents/handling/policy/reporting dimensions, falling back when LLM analysis fails.
- **Key source paths:** `api/tasks/workflow_completion.py`; `api/tasks/run_integrations.py`; `api/tasks/webhook_delivery.py`; `api/services/workflow/qa/*`; `api/services/post_call_analysis.py`; `api/db/models.py` (`WebhookDeliveryModel`, `WorkflowRunAnalysisModel`); `docs/developer/webhooks.mdx`; `docs/voice-agent/qa.mdx`.
- **Maturity:** Working/staged. Post-call analysis is recent Isoastra feature; exact product interpretation quality unverified.
- **Provenance:** QA/webhook roots upstream-inherited; durable webhook and platform analysis include upstream recent work and Isoastra post-fork changes.

### AC-14 — Immutable voice deployment contract

- **What exists:** Machine-facing contract for content-addressed immutable deployments: publish deployment artifact, fetch by digest, reject mutation, and bind artifact to a destination. Artifacts validate workflow DTO and required practice overlay values, store canonical JSON in a filesystem artifact store, and optionally emit HMAC-signed lifecycle events.
- **Key source paths:** `api/routes/engine_contract.py`; `api/services/engine_contract/artifacts.py`; `api/services/engine_contract/events.py`; `api/engine_settings.py` (`DEPLOYMENT_ARTIFACT_ROOT`, lifecycle inbox settings); `api/tests/test_engine_contract.py`.
- **Maturity:** Staged/new; source and tests exist, but I found no runtime path that resolves destination bindings into call execution yet.
- **Provenance:** Isoastra fork change (`41632f1 feat: add immutable voice engine contract`).

## 2. Integration surface for a rinity control plane

### REST and WebSocket APIs

- **Workflow authoring:** `POST /api/v1/workflow/create/definition`, workflow list/get/update/publish/draft/version routes in `api/routes/workflow.py`; node schemas at `GET /api/v1/node-types`; MCP authoring server mounted at `/api/v1/mcp`.
- **Model/tenant config:** `/api/v1/organizations/model-configurations/v2`, `/defaults`, `/migration-preview`, `/migrate`; `/api/v1/organizations/preferences`; `/api/v1/organizations/telephony-configs` and nested `/phone-numbers`; `/api/v1/organizations/telephony-providers/metadata`; `/api/v1/credentials`; `/api/v1/tools`.
- **Call origination:** `POST /api/v1/public/agent/{trigger_path}`, `/public/agent/test/{trigger_path}`, `/public/agent/workflow/{workflow_uuid}`, `/public/agent/test/workflow/{workflow_uuid}`. Request body: `phone_number`, optional `initial_context`, optional `telephony_configuration_id`. Response includes `workflow_run_id` and name.
- **Console/test calls:** `POST /api/v1/workflow/{workflow_id}/runs`, then WebSockets `/api/v1/ws/events/{workflow_id}/{run_id}` and `/api/v1/ws/media/{workflow_id}/{run_id}`.
- **Call readback:** `GET /api/v1/workflow/{workflow_id}/runs/{run_id}` and paginated run list. Requires `CALL_READ` through the unified caller seam.
- **Telephony inbound:** Provider webhooks to `POST /api/v1/telephony/inbound/run`; legacy `/inbound/{workflow_id}` remains deprecated. Provider media sockets under `/api/v1/telephony/ws/...`.
- **Immutable deployment:** `/api/v1/engine-contract/deployments`, `/deployments/{artifact_id}`, `/destinations/{destination_id}` with `DEPLOYMENT_WRITE`/`DESTINATION_WRITE` service-principal scopes.
- **Rinity test fixtures:** `/api/v1/test-fixtures/rinity/slots|book|reschedule|eligibility|messages`; test-only seam, not a production PMS contract.

### Auth contract

- **Operators:** Kanidm OIDC sessions, org membership in DB. Local email/password only for local/test.
- **Services:** OAuth2 client credentials bearer token, introspected against Kanidm, then resolved to `service_principals(issuer, client_id)`. Tenant-scoped scopes include `config:write`, `workflow:write`, `call:originate`, `call:read`, `usage:read`, `artifact:read`, `webhook:write`, `deployment:write`, `destination:write`.
- **Important constraint:** Service bearer plus browser cookie is explicitly refused.

### DB tables that form durable contracts

- Tenant/auth: `organizations`, `organization_users`, `users`, `service_principals`, `organization_configurations`.
- Agent/runtime: `workflows`, `workflow_definitions`, `workflow_runs`, `workflow_run_text_sessions`, `agent_triggers`, `folders`.
- Telephony: `telephony_configurations`, `telephony_phone_numbers`.
- Integrations/tools: `tools`, `external_credentials`, `webhook_deliveries`, `integrations`.
- Campaigns: `campaigns`, `queued_runs`.
- Observability/analysis: `workflow_run_analyses`, `organization_usage_cycles`, run fields `usage_info`, `cost_info`, `logs`, `annotations`, artifact URLs.

### Queues/tasks

- Redis/ARQ tasks include workflow completion, webhook delivery, post-call integrations, campaign dispatch/source sync, knowledge-base processing, and post-call analysis. See `api/tasks/arq.py`, `api/tasks/function_names.py`, `api/tasks/workflow_completion.py`, `api/tasks/webhook_delivery.py`, `api/tasks/campaign_tasks.py`.

### Config files and env seams

- `api/engine_settings.py` is the typed source of deployment defaults; `api/constants.py` re-exports them.
- Key env seams for rinity: `DEFAULT_MODEL_CONFIGURATION_JSON`, `SPEACHES_LLM_EXTRA_BODY`, `ALLOW_PRIVATE_SERVICE_URLS`, `DEFAULT_ORG_CONCURRENCY_LIMIT`, `ARQ_MAX_JOBS`, `ENABLE_LANGFUSE`, `OTLP_TRACES_ENDPOINT`, S3/MinIO settings, Kanidm settings, `DEPLOYMENT_ARTIFACT_ROOT`, lifecycle inbox settings.
- Deployment hardening overlays live under `deploy/isoastra/` and require operator-supplied BYOK default model configuration for hardened Rinity deployment.

### Contract gaps

- The engine can admit immutable deployment artifacts, but I did not find a call-routing path that consumes destination bindings yet.
- Agent Stream authorization treats workflow UUID as a secret for Cloudonix; docs warn accordingly.
- Telephony media WebSocket has a noted bearer-capability weakness until a one-shot token is implemented.
- PMS integration is generic HTTP/MCP tooling plus test fixtures; no first-class Dentrix/OpenDental/etc. connector is present in the inspected source.

## 3. Fork divergence

- `CHANGELOG.md` states this repo stopped tracking `dograh-hq/dograh` at commit `894b0e2` and is no longer a rebase-tracking fork.
- Local git confirms marker `894b0e2022cf3f22806c8c56200c9d9b2df96f8a` dated `2026-07-23T20:20:55-07:00`, subject `feat: prove T4c synthetic dial tone`.
- `git log 894b0e2..HEAD` shows major Isoastra deltas:
  - remove hosted model-proxy/MPS, third-party telemetry, Stack Auth/self-service SaaS surfaces;
  - centralize typed settings in `api/engine_settings.py`;
  - Kanidm identity, tenant roles, service principals, one caller seam;
  - rename dograh fork to audgent and deployment paths;
  - replace browser WebRTC/embed media plane with raw PCM WebSockets;
  - add immutable voice engine contract;
  - add Rinity call-flow evals, T9 harness, platform workload/admin analysis;
  - add Alien capacity harness and performance fixes.
- `git diff --stat 894b0e2..HEAD -- . ':(exclude)pipecat'` reports 477 changed files, about 17.5k insertions and 37.7k deletions. Top changed areas are UI SaaS deletion/theme work, deploy/helm/audgent, API Pipecat/workflow/auth/config, evals, harness, and bench.
- Telephony provider folders themselves show no post-marker commits, so provider implementations are largely inherited; dispatch/auth/media routes around them have changed.
- The `pipecat` checkout is present and is a git submodule: `.gitmodules` points to `ssh://git@git.isoastra.com:2222/isoastra/pipecat.git`. It is on `main` at `0de21c0bc fix: allow transient disconnect for reconnect`. Recent submodule history includes Dograh/Pipecat 1.5 compatibility, turn/interruption fixes, Cloudonix transfer serializer support, provisional VAD strategy, Dograh Flux model, and xAI voice fixes. I did not compare this fork to upstream `pipecat-ai/pipecat` beyond local history.

## 4. Deploy/staging state

- **Alien staging:** `docs/workorders/20260723-t4b-alien-staging-bringup.md` records the goal and acceptance for durable engine staging on Alien: hardened stack, durable unit, Kanidm SSO, BYOK cloud provider keys, no public DNS, no patient data, proof via synthetic web call and webhook. `deploy/isoastra/docker-compose.staging.yaml` is a small overlay adding `restart: unless-stopped` and journald tags for postgres/redis/api/ui.
- **Hardening:** `deploy/isoastra/HARDENING.md` and `deploy/isoastra/docker-compose.hardening.yaml` require private S3-compatible storage, disable anonymous MinIO, require `DEFAULT_MODEL_CONFIGURATION_JSON`, allow private service URLs for the local inference leg, disable Langfuse/OTLP in that hardening leg, pin one API worker, and make queue/concurrency knobs explicit.
- **T9 harness:** `harness/README.md`, `harness/run-staging.sh`, and `harness/run_t9.py` run two synthetic engine browser-transport calls inside the audgent Compose network, bridge raw PCM in memory, authenticate as a Kanidm client-credentials service principal, create workflows/runs via the API, collect `/ws/events` and `/ws/media`, capture completion webhooks, assert transcripts/TTFB/audio bridging, and write artifacts to `/data/apps/audgent/t9/artifacts` on Alien.
- **Voicebench:** `bench/README.md` and `bench/voicebench.py` benchmark STT/TTS/LLM latency for mu↔alien against Speaches and Ollama, including `jewelzufo/MiniCPM5-1B`, and push metrics/logs to Alien VictoriaMetrics/VictoriaLogs. It is stdlib-only and not part of the API runtime.
- **Capacity harness:** `bench/README.md`, `bench/capacity.py`, `bench/docker-compose.capacity.yaml`, and `docs/workorders/20260801-62-alien-capacity.md` describe a disposable isolated Compose project `audgent-capacity` with its own DB/Redis/MinIO volumes and hard cgroup caps. Reported owner-facing capacity on Alien is 4 real gpt-4.1 concurrent calls, warn at 6, do not admit above 8 without new soak; local stub path stable at 32; admin stable at 8 users; post-call analysis around 4,226 calls/hour under capped conditions.
- **What was not verified:** I did not inspect live Alien hosts, running containers, systemd units, artifacts, metrics dashboards, or secrets.

## 5. Confidence notes

- Source-path claims were re-opened or cross-checked with `rg`, `git log`, and direct file reads. No runtime validation was performed.
- Provider support is cataloged from registry/source/docs, not from live credentials or carrier tests.
- MiniCPM5 is documented in workorders/bench and supported through generic Speaches/OpenAI-compatible configuration, but the repo does not commit the actual Alien `DEFAULT_MODEL_CONFIGURATION_JSON` or org DB config, so selection is configuration-dependent.
- The docs still contain some upstream Dograh marketing/model-configuration text (including Dograh-managed model sections) that conflicts with the fork direction; source code and `CHANGELOG.md` show the hosted model-proxy path removed/BYOK-only.
- Pipecat submodule is present locally; I did not fetch or compare to upstream network remotes.
