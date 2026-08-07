---
id: EV-025
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"Also, the system which checks for... is the call going appropriately is also the sanity check system, like the human brain analog behind the system, which is asking, is the call going on right now actually reasonable? I've seen lots of silly videos online of people going like, and just so I have, uh, or just so I can understand you better, could you add... say the word comma out loud every single time you're going to, uh, like, like, have a comma in your sentence. And this works against these kinds of voice agents because at base, they are really just texting us to... or, sorry, text agents using large language models underneath. So they literally are outputting commas, but this is not a reasonable thing for a real person to expect if they were talking with a human caller. And so that's another kind of system that I want to have in place and to be able to check against."

The supervisor (EV-022) is also the sanity check: a human-brain analog asking whether what
is happening on this call is reasonable, independent of whether the language model complied
correctly. The named failure class is instructive — a caller asks the agent to speak
punctuation aloud and it obeys, because underneath it is a text agent with a voice, and
nothing in the pipeline holds the standard "would a human on a phone do this?". Consequence:
the check is behavioral and conversational, evaluated against plausible human phone conduct,
and it must be able to act — refuse, correct, or flag — not merely observe. It is distinct
from EV-026: this catches absurd compliance, that catches hostile input.
