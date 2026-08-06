---
id: EV-005
date: 2026-08-03
provenance: inference
source: agent design work in platform-scope.md, carried from live-site model; not yet built in Rust
---
Drafted identity model: identity / account / credential / auth_factor / session separation; humans as universal world entities (contacts ARE identity rows, no separate person table); role templates → project-scoped grants; capability links unify invites + feed tokens + claim links; audit log. Auth transferability contract (argon2id PHC, opaque hashed session tokens, no JWT/OIDC) is proven on the live site; the *universal-entity and grants model* is design, not yet validated by usage.
