---
id: EV-030
date: 2026-08-06
provenance: ground-truth
source: owner, data gate (session ③)
---
"4. if this isn't terribly expensive, we should have this, or self-host something, like how pelias is
self-hosted. For this product, IP is not actually important except for later anti-bot measures."

Answering the data gate's fourth escalation. The brief had recommended storing an IP and no city, on
the grounds that a city needs a GeoIP database — a new peripheral dependency for a console with one
user.

**Build it, self-hosted.** And it is far cheaper than the Pelias comparison implies: IP→city is a
memory-mapped `.mmdb` trie read in-process — no daemon, no port, no network call. Measured
2026-08-06: DB-IP City Lite is 61.7 MB gzipped (country-only is 4.1 MB), CC BY 4.0, monthly, no
account. It ships as a provisioned asset beside the hiqlite state rather than baked into the image,
behind an `IpLocator` that degrades to the raw IP when the file is absent or stale.

**Two things this settles beyond the immediate question:**

1. **The value here is small and the owner said so** — "IP is not actually important" for this
   product. It is built because it is nearly free and because the fleet will want the capability, not
   because the session list needs it.
2. **Anti-bot is now a named future consumer.** Together with the raw `fetch_count` that EV-029/DEC-014
   keeps for exactly this reason, the two inputs a bot-detection pass would want are being retained
   deliberately rather than rediscovered later as new work. It is not in this bet.
