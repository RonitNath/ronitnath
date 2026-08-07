---
id: EV-027
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"um, and also do cost optimization on these different bounds so I can know much this kind of system would actually cost me to use for different kind of purposes."

Cost is a measured product output: the owner wants to know what a call of a given shape
actually costs, per configuration, so the system can be evaluated for different uses.
Consequence: a cost ledger is part of the instrumentation from early on, attributed per
component and per provider/model configuration (EV-019), so cost comparisons and latency
comparisons come from the same runs. Together with EV-010 (latency, memory, CPU) this
completes the efficiency picture the evaluation surface (EV-013) must present. Note the
distinction from audgent/EV-013, which separates agent-test cost from production cost;
here everything is test, and the question is what production *would* cost.
