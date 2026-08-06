---
id: DEC-006
date: 2026-08-05
source: EV-018 (owner ruling)
---
Chose **SSE as the default transport for server-driven updates**, over polling and over WebSockets, on owner instruction: "prefer streaming updates via sse."

**Why SSE and not WS**: every case named is one-directional server→client — copy edits reaching open pages, RSVPs reaching the dashboard, identities reaching the directory. Writes stay ordinary POSTs. SSE rides plain HTTP with no upgrade handshake to get through the nanode Caddy ingress, and `EventSource` gives reconnect-with-`Last-Event-ID` for free, which is most of the correctness work in a live surface. WebSockets earn their complexity when the client streams too — that arrives with friend accounts and richer guest surfaces (EV-018's stated future), and the seam is being built now so that bet doesn't have to retrofit it.

**Why not polling**: the owner's stated experience of the surface is a dashboard held during a party. A poll interval slow enough to be cheap is slow enough to feel dead.

**Scope now**: F-3 (copy → all active pages for the event, guest pages included), F-6 (RSVPs → the event's admin panel), F-9 (identity changes → the people directory). Everything else stays request/response until it earns a stream.

**Costs taken deliberately, all landing on the data gate:**

1. **Fan-out and multi-instance.** One connection per open page. A single instance can hold a broadcast bus in process; more than one needs a shared channel, and PostgreSQL `LISTEN`/`NOTIFY` is the obvious candidate since the database is already there. Decide before the first horizontal scale, not after.
2. **Rolling zero-downtime upgrades break every stream.** Long-lived connections die when an instance drains — so a deploy is a mass disconnect, and correctness depends entirely on reconnect-and-resync working. This is the sharpest interaction in the bet: the operational posture the data gate must cover (PITCH bound, `system/data-gate.md`) and the transport choice are the same question.
3. **Stale-vs-disconnected must be visible.** A surface showing numbers that stopped updating is worse than one that never claimed to be live. Every streaming surface owns a disconnected state (F-6, F-9 `states`).
4. **Guest-tier fan-out.** Anonymous and tokened pages holding streams is a larger surface than owner-only; F-8's open question.

**What would change this**: if guest-side writes become frequent and interactive (live chat, collaborative editing), the calculus flips to WebSockets. Nothing in this bet does that.
