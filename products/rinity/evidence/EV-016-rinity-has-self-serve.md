---
id: EV-016
date: 2026-08-06
provenance: ground-truth
source: owner, PM interview (portfolio onboarding session 2026-08-06), topic 7
---
"Rinity has self-serve."

Settles a major product-surface requirement: a practice can sign up, configure, and get value
from rinity without an Isoastra human in the loop. This cuts against the current build in two
places: (1) the control plane admits only subjects pre-listed in `PRODUCT_MEMBER_SUBJECT_KEYS`
(EV-010/RF-02) — an allowlist door, the opposite of self-serve; (2) the audgent fork
deliberately deleted upstream's self-service SaaS surfaces (EV-011, fork-divergence) — correct
under EV-015, but the deleted capability (onboarding, billing, workspace management) must
reappear on the rinity side. Self-serve also implies pricing/billing surface — connects to
EV-019 (cash flow) directly.
