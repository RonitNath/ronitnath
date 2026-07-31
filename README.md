# ronitnath.com rebuild

A fresh derivation of `web_template` `public-site` at foundation revision
`48c1517`, rebuilt to retain the live anonymous visual language and copy.

## Included routes

- `/` — Ronit Nath landing page with social links.
- `/calendar` — current public calendar empty-state and browse affordance.
- Framework probes: public `/livez`, `/healthz`; separate operator `/readyz`,
  `/metrics`.

The landing page and calendar are Solid islands with server-rendered fallbacks.
Theme selection persists in browser storage; the responsive drawer is also a
Solid interaction. The existing starfield, typography, copy and token values
are retained from the live site.

## Deliberately deferred

This is the `public-site` composition, so it contains no database, login,
admin listener product routes or production bridge. The legacy event/invite,
guest, calendar-data/ICS, photos and mesh-admin surfaces are enumerated with a
safe follow-up sequence in [docs/dynamic-migration.md](docs/dynamic-migration.md).
That document also records the SQLx LF/checksum constraint for the later
persistence port.

## Development

```sh
pnpm install
pnpm run check && pnpm run test && pnpm run build
APP_ENV=development PUBLIC_ORIGIN=http://127.0.0.1:3000 cargo run
```

Run the Rust gates with:

```sh
cargo test --all-features --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
```

No deployment is performed or configured by this rebuild branch.
