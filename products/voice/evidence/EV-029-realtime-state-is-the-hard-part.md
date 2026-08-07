---
id: EV-029
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"But, again, the more important point is that there's a lot of independent systems going on here and a lot of different systems which are streaming their responses back, which means a lot of different kinds of state to manage in real time. And this is the biggest area where the console failed. It tried to put all the real time systems together at once, and I couldn't trace properly where the failure was happening. By building the system slowly and incrementally, I can ensure that all the baseline systems are hardened and then work my way up until there all the way until we reach the orchestrator, which is capable of running a full call with this full full stack. And then we can also understand what are the latencies, what is driving latency costs, and what kind of questions we need to answer about the underlying infrastructure in order to do scaling."

The thesis of the whole bet. The hard problem is not any one component but concurrent
realtime state across many independently streaming systems; console failed by composing all
of them at once, leaving failures untraceable. The remedy is the ladder: harden each
baseline, then compose upward until the orchestrator can run a full call over the full
stack. Consequences: (1) traceability is a first-class requirement — at every rung it must
be possible to localize a failure to a component, which means the tracing surface grows with
the ladder rather than arriving at the end; (2) "hardened" is the gate between rungs, and it
is the owner who judges it (EV-005, EV-009); (3) the orchestrator is the *last* thing built,
not the frame the others are hung on; (4) the payoff at the top is understanding what drives
latency and what infrastructure questions scaling actually poses.
