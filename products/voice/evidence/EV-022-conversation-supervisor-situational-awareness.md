---
id: EV-022
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"A second system I'm thinking about is one, which is able to just have a general idea of how the whole conversation is going and know is there dead time, what is the user's emotional state, are there other people in the background of the user's call, what are other sounds going on? Is it noisy? Uh, what is the user's intention? What other information might be useful to preempt for what the user might say in the nature"

[dictation artifact: "in the nature" = in the near future / next]

A supervisor that watches the whole conversation rather than the current turn, producing
situational signals: dead time, caller emotion, other people present, ambient sound, noise
level, intention, and predictions of what the caller may say next. Consequence: this is a
parallel non-blocking lane over the same timeline as the turn pipeline (EV-016), consuming
raw audio as well as transcript — several of these signals (background voices, ambient
sound, noise) do not exist in text and are lost if the supervisor only sees the transcript.
Its outputs feed the turn pipeline as context rather than gating it, and being "record-only"
first is what makes it safe to add without destabilizing the hardened rungs beneath. Same
system as EV-025 (sanity check), per the owner's own framing there.
