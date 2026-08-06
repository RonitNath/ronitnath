# Spec: data gate — owner session between the design gate and build

Purpose: the owner inspects **how base domain truth will be expressed and where the seams are** before code accretes around a wrong model. Artifact: a data-model brief at `changes/<pitch-tag>/data-model.md` in the code repo, reviewed with the owner.

## The brief must cover

1. **Domain model** — entities/tables, invariants, id discipline, ownership, lifecycle (create/mutate/export/delete/migrate).
2. **Access patterns** — the real read/write paths and hot paths the schema is shaped for.
3. **Interfaces & seams** — what is HTTP vs SSE vs WS vs RPC and *why*, per surface; internal vs public seams; contracts for machine actors.
4. **Operational posture** — elastic-scaling stance, rolling zero-downtime upgrade path, migration strategy and rollback.
5. **Peripheral services** — external requirements (LLM providers, notification channels, object storage, …) with their abstraction seam and failure behavior. State "none" explicitly when none.

## Loop-back rule

Iterating here may reveal additional UI surfaces. Those go back through the design gate (layers 3–4) and land as packet deltas — the gates are ordered but not one-way.

## Exit criteria

Owner signs off the domain model and seams. After sign-off, schema changes during the bet are deltas with reasons, same discipline as pitch and packets.
