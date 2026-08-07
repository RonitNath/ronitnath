---
id: EV-026
date: 2026-08-06
provenance: ground-truth
source: owner, audgent PM interview 2026-08-06 (flow review, diagnostics)
---
"Key diagnostic questions are being able to drill into specific packets and requests. This is to know, for instance, when a call failed, what respon[se] did the provider give which caused this? What did we send right before? Also with tool calls. This kind of system was useful to me when working with vapi because I could see that tool calls were failing because their schema was being rejected."

Adds a capability the interview had not surfaced, and it is the largest addition to the bet:
**wire-level inspection at every outbound boundary**. When a call fails, the owner must be
able to see the actual request/response pairs — to the provider and to tool endpoints — in
order, around the failure. The vapi precedent is a worked example of why: a tool call failing
because the endpoint rejected its *schema* is invisible at every altitude above the wire; the
transcript shows an agent that mysteriously didn't do the thing. Consequence: run
observability is no longer "transcript, recording, outcome, cost" — it descends to what was
sent and what came back, which is a data-volume and retention commitment (these payloads are
large, frequent, and may carry credentials or PHI). Note the failure mode this defends
against is *silent wrongness*, not an error page: the call completes, the agent sounds fine,
and the work never happened.
