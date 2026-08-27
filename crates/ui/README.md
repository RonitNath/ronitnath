# rn-ui

Shared Leptos CSR pieces for the three bundles: shell, table, form fields, API
client, subscription store, and `tokens.css` — the one stylesheet, linked by
every bundle's `index.html` and embedded by the askama pages.

## Looking at a bundle

Two terminals, from the repo root:

```sh
cargo run -p rn-ui --example fixture                  # the API, on :3199
cd crates/app-member && trunk serve \
  --proxy-backend=http://127.0.0.1:3199/api --proxy-ws  # the bundle, on :8080
```

Then open `http://127.0.0.1:8080/app` (`/org`, `/platform` in the other two
crates). The fixture serves `fixtures/whoami.json`, answers `/api/q/<name>`
from `fixtures/q/<name>.json`, echoes commands, and pushes one diff on
`/api/sub` followed by heartbeats.
