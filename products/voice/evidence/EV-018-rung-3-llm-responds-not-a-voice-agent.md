---
id: EV-018
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"And then after that, you have an LLM which responds. Now this doesn't need to be a full voice agent. At this point, we can just be at the level of, uh, you're talking to the voice agent... or sorry. You're talking to it in, uh, speech to text, and then the LLM is responding to you. And we can measure the latency between when the LLM starts responding and when you had stopped speaking or when end of turn was detected."

Rung 3: speak, transcribe, LLM replies in text. Explicitly *not* a voice agent — no speech
out, no turn-taking sophistication. The rung exists to make one number real: the gap from
the user stopping speaking (and, separately, from end-of-turn being detected) to the LLM's
first response token. Consequence: two distinct latency baselines are recorded here — true
speech offset and detected end-of-turn — and their difference is itself the quality measure
of the turn detector from EV-017. This rung is where the system first has a conversation
without yet having a voice.
