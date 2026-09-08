# Serving the sky data from the edge

*Investigation, 2026-09-08. Read-only except for one setting, named at the end.*

The sky is the heaviest thing this site ships: about 54 MB of star catalogues,
tiles, textures and one Milky Way bake, against roughly 1 MB for the rest of the
document. The question put was whether we could stop paying origin bandwidth for
it over and over — content-address it, lean on Cloudflare, get off "DO egress".

The short answer is that there is no DigitalOcean egress, the bandwidth bill is
not the thing that hurts, and the one real inefficiency is somewhere else
entirely: a client that asks for 64 KB of a tile makes the origin send the whole
file.

---

## 1. Where the bytes actually come from

There is **no DigitalOcean anywhere in ronitnath.com's serving path**, and no
object storage of any kind. Every byte of `/stars/*`, `/sky/*`, `/cities/*` and
`/textures/*` is baked into the application's Docker image and served by Next.js
out of `public/`.

    browser
      → Cloudflare (zone ronitnath.com, Free plan)
        → nanode `192.155.87.18` (Caddy, Linode 1 GB, us-west)
          → alien / delenda `:3140` over the wt0 mesh, `lb_policy first`
             (Next.js 15 standalone in Docker, `modules/ronitnath.nix`)

Confirmed against `~/dev/agentstate/ops/services/{ronitnath,edge}.md`, the live
`ronitnath.com` block in the nanode's `/etc/caddy/Caddyfile`, and the running
container `ronitnath-web-1` on both nodes.

DigitalOcean Spaces *is* used by this fleet, but by other projects (the
DentConnex referral attachments, the ClickStack backups, the isoastra.com Payload
media). None of it touches this site.

So the resources actually spent on a cache miss are:

| Resource | Allowance | Current use | Headroom |
| --- | --- | --- | --- |
| nanode egress to Cloudflare | 1 TB/month (Linode Nanode) | 307 GB TX in 54 days uptime ≈ **170 GB/month for every site on the edge** | ~83 % free |
| home cluster uplink (alien/delenda → nanode) | residential, shared with everything | same bytes, uncompressed (see §5) | the real scarce one |
| Cloudflare egress to visitors | free and unmetered on any plan | — | n/a |

Nothing here is metered in dollars. Origin fetches cost latency, the home
uplink, and the nanode's share of a 1 TB allowance we are using a sixth of.
**That reframes the whole exercise: this is a latency and tidiness problem, not
a bill.**

---

## 2. Zone facts (measured 2026-09-08)

Zone `ronitnath.com` = `d86abdaa3d47004d3af1487eb9edc6af`, plan **Free Website**.

- **Page Rules:** none (`/pagerules` returns an empty list).
- **Transform Rules:** none.
- **Cache Rules:** exactly one ruleset in the `http_request_cache_settings`
  phase (`860052eea2c7475e9746210b45895395`), holding one rule:

      expression: true
      action: set_cache_settings
      browser_ttl: { mode: respect_origin }
      description: "rn-site sets deliberate Cache-Control; do not override it"

  It sets only the *browser* TTL, and only to "respect origin". It does not
  change edge eligibility, does not force caching, and does nothing specific to
  `/stars/*`, `/sky/*`, `/cities/*` or `/textures/*` — all four are governed
  purely by the `Cache-Control` the app sends.
- **Cache Reserve:** unavailable. The API answers
  `1135: this zone setting is not available for your plan type`. It needs a paid
  plan.
- **Argo Smart Routing:** paid add-on, not enabled.
- **Tiered Cache:** `tiered_cache_smart_topology_enable` was **`off`**. It is now
  **`on`** — the one change this investigation made, see §7.

The scoped token in `cf-zone-edit.age` can read and write the `/cache/*`
settings but has no analytics permission, so Cloudflare's own request/cache
statistics were not available for this work.

---

## 3. How many origin fetches are we paying for?

**There is no usable request log, and that is the first finding.**

- The `ronitnath.com` Caddy block has no `log` directive, so the edge writes only
  errors for this host. `journalctl -u caddy` over seven days holds 613
  `http.log.access` lines mentioning the domain, but every one of them belongs to
  the `*.ronitnath.com` redirect block (`compoetry.dev.media.ronitnath.com` and
  friends, 308ing to the apex); apex requests appear only as the seven
  `http.log.error` records, mostly the 2026-09-08 05:36 `no upstreams available`
  burst.
- The Next.js container logs no requests: `docker logs ronitnath-web-1` is 34
  lines of startup and server-action errors.
- Cloudflare's GraphQL analytics are refused to this token.

So origin fetches were measured directly, by packet capture on the origin:

    ssh alien
    sudo nix run nixpkgs#tcpdump -- -i any -s 0 -n -w /tmp/rn.pcap 'tcp port 3140'
    sudo strings /tmp/rn.pcap | grep -aE '^(GET /stars|Range: |Content-Length: )'

Real requests are distinguishable from health checks: Caddy's active health
checker sends `Host: 100.88.39.223:3140`, while proxied traffic carries
`Host: ronitnath.com` and `Accept-Encoding: identity` (forced by
`header_up` in the Caddy block).

**Observed, 15-minute window, 2026-09-08 16:50–17:05 PDT, on alien (the
`lb_policy first` primary):**

| | count |
| --- | --- |
| `GET /healthz` (edge health checker + in-container check + the obs prober) | 110 |
| `GET /` | 1 |
| `GET /robots.txt` | 1 |
| **`GET /stars/*`, `/sky/*`, `/cities/*`, `/textures/*`** | **0** |
| requests carrying `Host: ronitnath.com` (i.e. proxied, not health-checked) | 32 |

**Zero bytes of sky data reached the origin in the window, and that includes a
real page load of `/`.** Every asset that visit needed was already at the SJC
PoP. Distinct tiles fetched from origin: 0. Origin sky bytes: 0.

A quarter of an hour is a small sample, but it is a *measurement*, and it points
one way: **the long tail is not being refetched, because there is barely any
traffic to refetch it for, and what traffic there is finds the edge warm.** With
volume this low the dominant risk is the opposite one —
Cloudflare's free-plan LRU evicts cold objects, so a 770-file tail on a
low-traffic zone is *mostly cold most of the time*, and a visitor's session pays
origin round trips for nearly everything it touches, at whichever PoP they land
on.

**First-principles bound, clearly labelled as an estimate.** The client is paced
to a 640 KB burst plus 5 KB/s, budgeting ~6 MB of sky over five minutes
(`lod-stream.ts`). A session that lands on a completely cold PoP therefore costs
the origin *at most* the full size of every tile it touched — and because of §4,
that is up to 5× the client's own byte budget on dense tiles. Call it **10–20 MB
of origin traffic per fully-cold session**, and near zero for a session on a warm
PoP. Even at a hundred cold sessions a month that is 1–2 GB — around 1 % of what
the nanode already moves.

---

## 4. Ranged requests: the actual finding

`lod-stream.ts` `Range`-fetches only the first ~64 KB of a magnitude-sorted tile,
on the reasoning that "paying 324 KB to draw 64 KB is how the whole budget goes".
That reasoning holds for the client. **It does not hold for the origin.**

Method: request a guaranteed-cold cache key (the real file plus a random query
string, which changes Cloudflare's cache key without changing what the origin
serves) while capturing on the origin.

Client side, `https://ronitnath.com/stars/lod/215.bin?x=<random>`,
`Range: bytes=0-65535`:

    HTTP/2 206
    content-range: bytes 0-65535/272956
    cf-cache-status: MISS

Origin side, same moment, entire capture:

    GET /stars/lod/215.bin?x=r2083410273 HTTP/1.1
    Content-Length: 272956

**One request, no `Range` header, the whole 273 KB file.** Cloudflare fetches the
complete object, stores it, and slices the range out of its own copy. Proof that
it really is stored whole: a request for a *different* range of the same key
seconds later returns `206 … bytes 131072-196607/284588` with
`cf-cache-status: HIT` and no second origin fetch.

Two secondary behaviours worth writing down, because they cost an hour to
untangle:

- For roughly the first ~30 seconds after a fill, Cloudflare may answer a `Range`
  request with a **full `200`** carrying the entire body, ignoring the range,
  even while reporting `HIT`. Once the object settles it serves `206`
  consistently. Observed on three separate cold keys.
- A request with **no `Accept-Encoding` header at all** also got the full `200`.
  Real browsers always send one, so this is a curl artefact, but it will confuse
  anyone testing by hand. Test with
  `-H 'Accept-Encoding: gzip, deflate, br, zstd'`.

So the tile design is cheap for the visitor and for Cloudflare's egress, and is
*not* cheap for the origin: on a cold PoP, every 64 KB slice costs a full-file
fetch. The dense tiles are 270–325 KB, so origin bytes run about 4–5× the
client's accounted budget.

---

## 5. Content addressing, compression, and the manifest

The content-addressing work landing alongside this (`asset-names.ts`,
`tools/starcat/name_assets.py`, and the widened `next.config.ts` header rule) is
**the right shape**. `<id>-<hash12>.bin` is exactly what an edge wants: the URL is
the version, so `immutable` is honest, a rebake is a new key, and no purge is
ever needed. Nothing about it needs changing.

What it fixes, measured before it landed: `bright.bin` (380 KB), the Milky Way
WebP (315 KB), `cities.bin` (98 KB), `lines.bin`, `named.json` and the three
Earth textures were all `public, max-age=0` and came back `REVALIDATED` from
Cloudflare — an origin round trip per page load.

What it does **not** fix, and this is worth being precise about: `REVALIDATED`
was never costing much *bandwidth*. Verified straight against the origin —

    curl -H 'Host: ronitnath.com' -H 'If-None-Match: W/"5f3e8-1a07e261d5e"' \
         http://100.88.39.223:3140/stars/bright.bin
    → HTTP/1.1 304 Not Modified

Next.js answers the conditional GET with a 304 of a couple of hundred bytes.
Content addressing removes ~1.7 MB of *revalidation round trips per page load* —
a real latency win, and the right thing — but it was never 1.7 MB of egress.

**Compression.** Nothing compresses the `.bin` files today, at any hop, and that
is correct:

| file | raw | gzip -6 | zstd -19 |
| --- | --- | --- | --- |
| `stars/lod/280.bin` | 324,460 | 283,272 (87 %) | 281,437 (87 %) |
| `stars/bright.bin` | 390,120 | 291,869 (75 %) | 279,229 (72 %) |
| `cities/cities.bin` | 100,258 | 71,632 (71 %) | 68,266 (68 %) |
| whole `stars/lod/` | 51,866,860 | 45,487,045 (88 %) | — |

13 % on the tiles, for CPU on every hop and a `Vary: Accept-Encoding` split of
the edge cache key. Caddy's `encode zstd gzip` only matches text-ish content
types, so `application/octet-stream` passes through untouched, and Cloudflare
serves the tiles with no `Content-Encoding` and no `Vary` at all. **Leave it
alone. Do not enable compression on `.bin`.** `named.json` and the tile manifest
*are* gzipped and carry `Vary: Accept-Encoding`; that is fine for JSON.

**ETag and Vary.** The tiles are served with a weak `ETag` and no `Vary`, which is
the single cleanest cache key you can have. Once everything is `immutable` the
ETag stops mattering entirely.

**The manifest indirection is fine.** `/stars/lod/manifest-<hash>.json` is 110 KB,
content-addressed, gzipped by the edge, fetched with `cache: 'force-cache'`, and
its hash changes whenever any tile does. It costs one extra round trip before the
first tile and nothing after. The only thing to watch is that a *single byte*
changing in *any* tile changes the manifest's hash, so every rebake invalidates
the manifest — which is correct behaviour, just worth knowing it is not
incremental.

---

## 6. Options

Deploy figures, measured on alien: image `ghcr.io/ronitnath/ronitnath:37a55727`
is 311 MB; `docker save | wc -c` is **316.6 MB**, which is what crosses the mesh
to delenda on every single deploy. The pre-sky image `d81e1327` was 259 MB, so
**the sky data is 52 MB, 16.4 %, of every deploy transfer.**

| | What it does | What it costs | Verdict |
| --- | --- | --- | --- |
| **(a) Content addressing + `immutable`** *(landing anyway)* | Kills ~1.7 MB of revalidation round trips per page load; makes every asset purge-free. | Nothing. Already done. | **Ship it.** Necessary, and it is the largest latency win available. |
| **(b) Tiered Cache (Smart Topology)** | Lower-tier PoPs fill from an upper tier instead of from origin, so the 770-file tail is fetched from alien roughly once rather than once per PoP. | Free on Free plan. One API call to reverse. | **Done.** See §7. Directly attacks the per-PoP refill that §4 makes expensive. |
| **(c) Cache Reserve** | Persistent backing store so evicted objects never fall back to origin. | Not available on Free (measured: error 1135). Needs a paid plan, plus $0.015/GB-month and per-op fees. | **No.** (b) covers the same failure mode for free, and 52 MB is not worth a plan upgrade. |
| **(d) Move `/stars/*` etc. to R2 on a second hostname** | Origin egress for sky data → zero. Image sheds 52 MB, deploy transfer 316.6 → ~264 MB (−16 %). R2 egress is free; 52 MB sits inside R2's 10 GB free tier, so ~$0/month. | A second hostname; assets no longer versioned atomically with the app; a publish step in every release; a CORS decision (`crossorigin` on the fetches, `Access-Control-Allow-Origin` on the bucket); one more thing that can be out of sync during a rollback. | **Not yet.** It solves a bandwidth problem we do not have. Revisit if traffic grows an order of magnitude or the home uplink becomes the complaint. |
| **(e) Push to GHCR instead of `docker save \| ssh`** | Fixes the *deploy transfer* half of (d)'s benefit without splitting the assets off: the registry dedupes layers, so an unchanged 52 MB sky layer is never re-sent to delenda. | The repo already names `ghcr.io/ronitnath/ronitnath` and the note says "No GHCR push yet". Needs a pull credential on both nodes. | **Better than (d) for the deploy cost.** Recommended as a separate, independent change. |
| **(f) Coarser tiles** | 770 files is a long tail with 770 chances to be cold. Fewer, larger tiles fill faster and evict less often. | The 64 KB range trick already gets brightest-first partial reads, so bigger tiles do not cost the client more — but §4 means bigger tiles cost the *origin* more per cold fetch. | **No.** Given (b), the tail is now filled through one upper tier; making tiles bigger would raise the per-cold-fetch origin cost that §4 identified. If anything the granularity is right. |
| **(g) Ask the origin for the range** | Have Cloudflare forward the client's `Range`. | Not configurable on any plan; Cloudflare always fills the full object. | **Impossible.** Recorded so nobody tries. |

---

## 7. What was changed

**Smart Tiered Cache Topology, off → on**, on zone
`d86abdaa3d47004d3af1487eb9edc6af`, at 2026-09-08T23:49:47Z.

It is free on the Free plan, changes nothing about what is served, and only
reduces how many PoPs fetch from origin. It is the single highest-value change
available given §4: a cold-PoP fill costs a full-file origin fetch, and tiered
cache is what stops that from happening once per PoP.

Before:

    {"id":"tiered_cache_smart_topology_enable","value":"off","editable":true}

After:

    {"id":"tiered_cache_smart_topology_enable","value":"on","editable":true,
     "modified_on":"2026-09-08T23:49:47.337307Z"}

To reverse it, one call:

    ZID=d86abdaa3d47004d3af1487eb9edc6af
    T=$(age -d -i ~/.config/age/keys.txt ~/keys/cf-zone-edit.age)   # nexus only
    curl -s -X PATCH \
      -H "Authorization: Bearer $T" -H 'Content-Type: application/json' \
      -d '{"value":"off"}' \
      "https://api.cloudflare.com/client/v4/zones/$ZID/cache/tiered_cache_smart_topology_enable"

(The `cf-zone-edit.age` token is scoped to nexus. It can write `/cache/*` but has
no analytics or zone-settings read.)

---

## 8. Recommendation

**Ship the content addressing, keep Tiered Cache on, and do nothing else about
the CDN. Then, separately, move deploys to GHCR.**

Cost of this recommendation: zero dollars, zero new hostnames, zero new failure
modes. Everything it buys is already bought.

The premise the investigation started from — that we are burning metered egress
on repeated fetches of static sky data — does not survive measurement. There is
no DigitalOcean egress; the nanode is at a sixth of its transfer allowance
serving the entire fleet; and a 15-minute origin capture caught no organic sky
traffic at all. R2 would be a correct answer to a bandwidth bill we are not
receiving, at the price of splitting the assets away from the release that
produces them.

What *is* real, and what the measurement actually found, is that the tile
streamer's 64 KB range reads cost the origin whole 270–325 KB files on a cold
PoP. Tiered Cache is the free fix for that, and it is now on.

If the picture changes — traffic grows enough that the home uplink is felt, or
the sky data grows past a few hundred megabytes — the order to reach for is:
(e) GHCR deploys first, because it is cheap and independent; then (d) R2, at
which point the CORS and release-publish costs start to look worth paying.

### If the owner wants to adopt (d) anyway

The exact shape, so the decision is a decision and not a design exercise:

    # 1. bucket + custom hostname (sky.ronitnath.com), Cloudflare in front
    wrangler r2 bucket create rn-sky
    wrangler r2 bucket domain add rn-sky --domain sky.ronitnath.com

    # 2. publish step, added to deploy/README.md, run before the image is built
    wrangler r2 object put rn-sky/stars/lod/... --file public/stars/lod/... \
      --cache-control 'public, max-age=31536000, immutable'

    # 3. CORS on the bucket — the fetches are same-origin today and would stop being so
    #    Access-Control-Allow-Origin: https://ronitnath.com
    #    and `crossorigin` on the <img>/fetch for the WebP and the textures

    # 4. asset-names.ts gains an origin prefix; next.config.ts's header rule
    #    becomes dead for the moved paths

At R2's published pricing (check it before committing): storage 52 MB, inside the
10 GB/month free tier; egress to the internet free; reads inside the 10 M/month
Class B free tier. Effectively $0/month, and the deploy transfer drops from
316.6 MB to about 264 MB.

### If the owner wants (e)

Build on alien as today, then `docker push ghcr.io/ronitnath/ronitnath:<sha>` and
`docker pull` on delenda instead of `docker save | ssh delenda docker load`.
Unchanged layers — the 52 MB sky layer among them — are never re-sent. Needs a
GHCR pull credential on both nodes; `ronitnath.md` already records that the push
half is not set up yet.
