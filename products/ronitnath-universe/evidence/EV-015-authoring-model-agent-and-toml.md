---
id: EV-015
date: 2026-08-05
provenance: ground-truth
source: owner, design-gate review of the flow boards
---
The authoring model, stated directly: "the event is created via me talking with an ai coding agent which authors the pages and flows. Editing the copy becomes an act of me just going through a toml file which defines copy for the event and directly doing editing there. If I want structural changes, I'd talk to the agent."

Three journeys, three mediators:

| Change | Path | Surface |
| --- | --- | --- |
| Create the event | conversation with the coding agent → agent authors pages, flows, components | outside-web |
| Change wording | owner edits the event's `copy.toml` directly | outside-web (a text editor) |
| Change structure | conversation with the coding agent | outside-web |

This **supersedes the inline-copy-editing surface** in PITCH-001 §4 and PKT-05 acceptance ("edit text props of any module inline"). The refinement surface the owner asked for at EV-012 is a *file*, not a form. What remains for the web UI is the data that only exists at runtime: RSVPs, guests, invite links, publish state.

Open question this forces (data gate): the copy file has to reach production. Compiled in (every typo is a rebuild + redeploy), read from disk at runtime, or stored in the DB and edited through a plain TOML text box in the console — the last preserves "I just edit a toml" while keeping the edit-to-live path a single action. Unresolved here deliberately; it is a seam question.
