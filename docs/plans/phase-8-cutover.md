# Phase 8 — Cutover (operator, by hand)

Not delegated. Sequence:
1. Deploy compose on nexus (site :3130 on 10.0.0.1 + 127.0.0.1, admin :3131
   on 100.88.31.199 + 127.0.0.1). Import prod DB for real (copy aside
   first).
2. nanode /etc/caddy/Caddyfile: ronitnath.com -> 10.0.0.1:3130 (+ NetBird
   fallback 100.88.31.199:3130), replacing gateway upstreams;
   events.ronitnath.com -> 308 https://ronitnath.com{uri} (preserves
   /e/{token}); wildcard stays; update hive ingress/ronitnath.com.caddy
   reference copy.
3. Smoke: home, real invite links, RSVP, admin over mesh.
4. Only after verification: stop gateway container + events compose, retire
   hive ronitnath.nix, archive ronitnath-legacy on GitHub, rewrite
   context/ronitnath/events.md, delete completed plans per orchestration.md.

Rollback: revert the Caddy block (gateway container intentionally left
running until step 4).

Note: T2->T1 loses no availability today (delenda down => T2 already
single-host). Promote back to T2 when the pair is restored.
