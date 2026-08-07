---
id: EV-016
date: 2026-08-06
provenance: ground-truth
source: owner, audgent PM interview 2026-08-06 (dictated)
---
"I think that the underlying system is pipe[cat], and my intention is that we rely on the frames of pipecat in order to effectively implement this system without having to do a lot of internal work on the platform."

Names the implementation seam and the appetite together. Phonetic repair (EV-015) rides
pipecat's frame pipeline — a processor in the stream — rather than surgery on audgent's
workflow engine, routes, or data model. Consequence: the change lands mostly in the
`isoastra/pipecat` submodule and the pipeline builder, which is also the cheapest thing to
carry forward into `voice` or abandon with audgent (EV-007). A proposal that requires broad
audgent-internal rework to deliver this capability is contradicting an owner ruling, not
making an engineering tradeoff.
