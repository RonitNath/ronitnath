# rn-ui

Shared Leptos CSR pieces for the three bundles: shell, table, form fields, API
client, subscription store, and `tokens.css` — the one stylesheet, linked by
every bundle's `index.html` and embedded by the askama pages.

## Looking at a bundle

Two terminals, from the repo root:

```sh
cargo run -p rn-ui --example fixture                        # the API, on :3199
cd crates/app-member && trunk serve --public-url / --serve-base /
```

Then open `http://127.0.0.1:8080/app` (`/org`, `/platform` in the other two
crates). The proxies to the fixture live in each bundle's `Trunk.toml`; the two
flags override the production `public_url` so trunk serves the app at the tier
path instead of under `/app/pkg/`. The fixture serves `fixtures/whoami.json`,
answers `/api/q/<name>` from `fixtures/q/<name>.json`, echoes commands, and
pushes one diff on `/api/sub` followed by heartbeats.
