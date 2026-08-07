---
id: EV-015
date: 2026-08-06
provenance: ground-truth
source: owner, audgent PM interview 2026-08-06 (dictated)
---
"the phonetics are essentially just because there are several times when the user is speaking and the speech to text system misinterprets what they were saying. And I want the agent to be able to be intelligent enough to try to interpret what the user was saying in the first place. For instance, there was a time when I said, 'what are the times available?' but it heard 'water the times available' […] the agent […] could try to use those to deduce what the person was saying. I've explored some […] services which give you the full kind of breakdown of the rank[ed] options for what it could have been, but that's not great because I don't wanna span [spend] the agent's tokens. The agents have a generally good sense of what IPA corresponds to what words, and so should be able to deduce based on that information."

Gives EV-003 its purpose and rejects the obvious alternative. The goal is **misrecognition
repair**: when STT produces a plausible-but-wrong transcript ("water the times available"),
the voice agent should recover the intended utterance instead of answering the wrong
question. The mechanism is a compact phonetic representation — IPA the LLM already knows how
to read — attached to the transcript. Explicitly rejected: n-best / full ranked-alternatives
output from STT vendors, on token cost. Consequence: the deliverable is a phone-level signal
narrow enough to sit in the prompt on every turn, which is a much tighter constraint than
"add forced alignment"; and the win condition is measured in recovered utterances, not in
alignment accuracy.
