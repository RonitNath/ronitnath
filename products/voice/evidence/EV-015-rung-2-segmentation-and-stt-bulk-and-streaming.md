---
id: EV-015
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"The next level is doing really fine segmentation of these clips and then sending it to some kind of speech to text provider and thus gaining transcripts. There is the bulk path, and there's also the streaming path."

Rung 2: fine-grained segmentation of stored audio, then transcription. Two paths are named
as distinct and both in scope — bulk (whole-clip, offline, exact) and streaming (incremental,
realtime, partial). Consequence: the segmentation layer is a first-class component with its
own quality bar, not a hidden preprocessing step, and the STT seam must express both modes
rather than pretending streaming is bulk with a callback. The bulk path is what makes the
streaming path measurable: the same clip through both yields a reference transcript to
score partials against.
