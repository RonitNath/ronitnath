---
id: EV-029
date: 2026-08-06
provenance: inference
source: read-only source check of isoastra/audgent @ 312ece2 on nexus (~/dev/isoastra/audgent), run by the PM session directly. No builds, servers, DB queries or network calls.
---
Answers BRIEF-001/OQ-1, OQ-6 and OQ-8 from the source. Inventory, not strategy. Complements
rinity/EV-011 (the capability catalog) at the specific points this bet commits to.

# Source check — change history, wire capture, tenancy blast radius

## 1. Configuration change history (OQ-6) — does not exist, with one exception

- `api/db/models.py` defines 24 models. **None is a change log, audit table, or event table.**
  `rg -i audit` over `api/` returns only unrelated hits: a workflow-definition lint
  (`api/services/workflow/audit.py`, rule-based node/edge checking) and comments labelling
  `created_by` columns as "audit fields".
- `created_by` exists on workflow definitions, campaigns, external credentials, tools,
  knowledge-base documents and recordings — **creation attribution only**. There is no
  `updated_by` or `changed_by` column anywhere in the schema.
- **The exception is workflows.** `WorkflowDefinitionModel` is genuinely versioned:
  `status` (draft/published/archived), `version_number`, `published_at`, `is_current`, and a
  full behavioral snapshot (`workflow_json`, `workflow_configurations`,
  `template_context_variables`). Runs pin the definition they ran. So *workflow* change
  history is real and reconstructable.
- Everything else — provider/model configuration (`organization_configurations`), credentials,
  tools, telephony configurations, phone numbers — is **mutated in place with no history**.

**Consequence:** F-5 (what the agents have been up to) is new construction, and it is new
construction over exactly the configuration space EV-025 says matters most — provider
selection. Workflow edits could be reconstructed from existing versioning; provider changes
could not be reconstructed at all, because the prior value is overwritten. This is a schema
commitment, and it must land *before* agents start changing things, since history cannot be
backfilled.

## 2. Wire-level capture (OQ-8) — partial, and asymmetric the wrong way

**Tool calls — response captured, request not.**

- `rtf-function-call-start` events carry `function_name`, `tool_call_id` and **`arguments`**
  (the model-supplied ones). `rtf-function-call-end` carries the result, `str()`-ified
  (`serialize_realtime_feedback_tool_result`).
- The result dict from `execute_custom_tool` contains `status`, **`status_code`**, and
  **`data`** — the parsed response body, or `{"raw_response": <text>}` when it isn't JSON —
  plus `request_headers` when `include_request_headers` is set. Errors return
  `{"status": "error", "error": ...}` for timeouts, request errors and unhandled exceptions.
- These events persist to `workflow_runs.logs.realtime_feedback_events` and are already
  rendered in the run detail page (`ui/src/app/workflow/[workflowId]/run/[runId]/page.tsx`
  counts `il-end` events as tool calls).
- **What is missing is the request.** The URL, method, resolved headers and body are written
  only to the Python logger — `logger.info(f"Executing custom tool ... {method} {url}")` and
  `logger.debug(f"Request body: {body}, params: {params}")` in
  `api/services/workflow/tools/custom_tool.py`. Nothing per-run. So EV-026's "what did we send
  right before?" is **not answerable from a run today**, while "what did the provider give
  back?" largely is.

**Provider (STT / LLM / TTS) exchanges — not captured per-run at all.**

- The only mechanism is OTEL spans exported to Langfuse or a plain OTLP endpoint
  (`api/services/pipecat/tracing_config.py`), and `enable_langfuse` defaults to `False`
  (`api/engine_settings.py:351`). Spans are stamped with `dograh.org_id` and routed
  per-org — machinery that becomes vestigial under EV-017.
- `workflow_runs.logs` carries transcription, bot text, node transitions, TTFB/latency
  metrics and pipeline errors — conversation-altitude events, not wire-altitude ones.

**A security finding, unasked for but load-bearing:** `log_level` defaults to **`DEBUG`**
(`api/engine_settings.py:370`). Tool request bodies are therefore *already* being written to
container logs — unstructured, uncorrelated to a run, and flowing wherever journald/Victoria
Logs collect. The payloads EV-026 wants to capture deliberately (with retention and redaction
decided) are partly being captured accidentally today, with neither. Whichever way OQ-8 is
answered, this default should be revisited in the same breath.

## 3. Tenancy blast radius (OQ-1)

- `organization_id` appears on **41 model columns** across `api/db/models.py`, and in
  **202 Python files** under `api/`.
- The migration chain has **98 alembic revisions**.

**Consequence:** removing org scoping is a wide change, not a deep one — mechanical, spread
across most of the codebase, and touching every table. Degenerating instead (pin a single org,
stop surfacing it) is dramatically cheaper and reversible; genuine removal buys a smaller
schema and no vestigial concept. EV-019 (no live traffic) makes either affordable; the cost
argument now favors degeneration, and EV-023's long horizon is the counter-argument for doing
it properly. This is the decision the debate should take.
