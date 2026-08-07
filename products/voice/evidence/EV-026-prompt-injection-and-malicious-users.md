---
id: EV-026
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"I also want to check against prompt injection and malicious users"

Adversarial callers are in scope as a system, not as a prompt-hardening afterthought. The
threat arrives over the audio channel and reaches the model as transcript, so the defended
boundary is inside this product rather than in the consuming product (EV-012). Consequence:
a security seam belongs in the design — what the caller can influence, what they cannot,
and what happens when injection is detected — and `procedures/security.md` applies to this
bet. Pairs with EV-025: absurd-but-benign compliance and deliberate attack are different
detections sharing one supervisory lane.
