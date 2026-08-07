---
id: EV-011
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"In terms of console, I don't want to reference it at all. This is a greenfield build. However, maybe there are some useful concepts like, uh, how to do browser connection or something that might be useful, but you can also get these things from AUD GMT or other production services."

[dictation artifact: "AUD GMT" = audgent]

Console is not prior art to harvest; it is discarded. The exception is narrow and named:
generic integration knowledge such as browser audio connection, and even that is available
from audgent or any production service, so console holds no privileged position. Consequence:
no code, no crate layout, no schema, and no design is carried across; workers must not read
console for guidance. This is stronger than the "absorb by feature rebuild, never port"
doctrine used elsewhere in the portfolio — there is not even a feature set being absorbed.
It also removes the tempting shortcut of starting from console's frame core, which is
exactly the "run before you can walk" move EV-009 rejects.
