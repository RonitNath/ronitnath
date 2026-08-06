---
id: EV-006
date: 2026-07-21
provenance: ground-truth
source: owner ruling, captured as the 2026-07-21 amendment in ~/dev/context/isoastra/business-direction.md; expanded in ~/dev/context/isoastra/products-history-20260724.md
---
"ai-assistant is entirely retired. The voice-agent product is a fresh rinity platform on web_template with the audgent fork as engine."

Settles the two-plane architecture rinity was built on: a web_template control plane
(`isoastra/rinity`) plus the audgent engine (`isoastra/audgent` + `isoastra/pipecat` forks,
MiniCPM5 swap, voicebench harness, alien staging). ai-assistant is readable prior art only
(cap_* auth, pack config layering, voice_traces schema) — never rinity's substrate. voce is
the parked long-term native-engine path. Qualified by EV-009: the two-plane split itself is
now revisitable, though audgent remains the expected engine target (EV-008).
