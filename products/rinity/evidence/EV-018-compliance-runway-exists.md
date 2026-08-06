---
id: EV-018
date: 2026-08-06
provenance: ground-truth
source: owner, PM interview (portfolio onboarding session 2026-08-06), topic 7
---
"We already have compliant DO nodes (nexus, sfo, nyc) and all model providers offer hipaa compliant model hosting once the product is ready for a first real non-test non-demo business"

Settles that compliance is a runway, not a blocker: HIPAA-capable infrastructure (three
DigitalOcean nodes) already exists, and HIPAA-compliant model hosting is available from every
model provider when needed. The trigger for actually assuming that posture is "first real
non-test non-demo business" — until then, staging/demo posture (no patient data, per the
audgent alien-staging rules in EV-011) is acceptable. Design consequence: the architecture
must not create compliance debt that makes the eventual cutover hard (data residency, BAA-able
providers, PHI flow isolation), but the bet does not start in compliance mode.
