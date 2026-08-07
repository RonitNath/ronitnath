---
id: EV-016
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"being able to scrub through a transcript and that aligning with the... or, sorry, being able to scrub through the replay and that aligning with the transcript, having the waveform to be able to see what actually was said and how it sounded."

The rung-2 inspection surface, stated as a requirement rather than a nicety: a scrubbable
replay whose position is bound to the transcript, with a waveform beside it. The stated
purpose — "see what actually was said and how it sounded" — is the direct answer to the
console theater problem (EV-009): the owner wants the raw evidence next to the machine's
claim about it, so a wrong transcript is visible as wrong. Consequence: time alignment
between audio, transcript tokens, and any derived annotation is a structural property of
the data model, not a view-layer convenience; every later signal (turn events, phonemes,
diarization, LLM timings) hangs on the same timeline.
