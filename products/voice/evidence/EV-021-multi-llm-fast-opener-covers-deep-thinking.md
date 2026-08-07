---
id: EV-021
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"First is having multiple LLMs going, and the purpose of this is to have an early beginning response that is quick while the main media response, which requires thinking and tool calling, can be happening in the back end. This way, the first couple of spoken words, essentially, are providing, uh, some cover for the rest of the deep thinking to be done for it to be produced. Since every layer of this is streaming and the tokens are streamed out, as soon as the main model starts responding, it it can pick up on the stream of what was said before, but we just have to be careful about how to design this."

[dictation artifact: "main media response" = main/mediated response, i.e. the primary model's answer]

The first of the systems above the base voice agent: two models on one turn, a fast one
emitting the opening words to buy latency cover while a slower thinking/tool-calling model
produces the real answer, then a handoff where the main model continues from what has
already been spoken. The owner flags the design risk himself — the seam must not produce a
contradiction, a repeat, or an audible seam. Consequence: the turn's output is a single
stream with multiple producers, so the orchestrator needs an explicit handoff contract
(what the second model is told was already said, and what it may not retract) and the
measurement rig must show the join point. This is a named reason the architecture must be
streaming end to end (EV-029).
