---
id: EV-042
date: 2026-08-06
provenance: ground-truth
source: owner, design gate ② round-3 review of Penpot "rinity pitch-rinity-1"
---
On the active call header's LIVE badge and inline elapsed time: "remove the 'live' and
time - those read as unintentional."

On the waveform panel's LIVE label: "also remove this live. The fact that it grows is
enough evidence of it being live. Include the call ongoing duration in the top right."

On the call detail's outcome chip: "remove this booked from the call detail boards."

"Make a proper page for editing the knowledge base. The KB isn't just one long text
field. Consider from first principles how to design the KB well."

Spec consequences (beyond the mock):
- **Liveness is evidenced by motion, never labeled.** A live surface shows growing
  content (streaming transcript, advancing waveform); no LIVE badge anywhere. The
  ongoing call duration sits top right (→ PKT-01 design language, PKT-10). Extends
  interface-minimalism: a LIVE label is the UI explaining itself.
- **Call detail carries no outcome chip.** Outcome is queue-row metadata and is
  evidenced on the detail page by the commitments panel, not restated as a badge
  (→ PKT-05).
- **The knowledge base is a structured surface, not a text blob**: a dedicated
  settings sub-page of per-topic free-text entries, each independently editable in
  place, each carrying provenance (onboarding scrape vs person who added it), with
  the agent's captured unanswered caller questions surfaced beside the entries so
  answering one grows the KB. First-principles rationale: entries are the unit the
  agent actually recites; provenance is already required per-field by PKT-07; the
  capture queue is real witnessed data from PKT-03's no-improvising rule
  (→ PKT-03).
