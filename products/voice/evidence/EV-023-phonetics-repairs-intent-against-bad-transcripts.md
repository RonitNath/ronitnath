---
id: EV-023
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"and then also phonetics to be able to discern what the user was saying based on their intention even if the transcription system produce words that seem kind of nonsensical or against the intent of what the caller seemed to be requesting."

Phonetics as a repair lane: when the transcript is nonsensical or contradicts the caller's
evident intent, phone-level evidence plus intention (EV-022) recovers what was actually
said. Consequence: phonemes are a retained first-class signal on the timeline, not a
debugging aid — the system must keep enough sub-word acoustic detail to re-decide a word
after the STT provider has already committed to it. This is the same capability audgent
pursues as its bet 2 (audgent/EV-015, EV-016) and the frontier capability named at
audgent/EV-003; here it is native to the design rather than grafted onto a provider
abstraction that hides the audio plane.
