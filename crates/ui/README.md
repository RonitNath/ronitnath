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

## Disk, or the copy in the binary

A bundle is served twice over. `rust-embed` bakes each `crates/<bundle>/dist`
into the server binary at compile time, and the same paths are read from disk
at run time; which one wins is the mode (`crates/server/src/assets.rs`). Dev
reads disk first, so a `trunk build` is visible on reload. Release reads the
embedded copy first, so a node serves its whole frontend out of one binary and
a half-copied image cannot produce a page that half-works.

Two consequences worth knowing before you spend twenty minutes on a stale page:

- **In dev, `dist/` is the truth.** A change to a bundle needs `trunk build
  --config crates/<bundle>/Trunk.toml` before the running server will show it.
  `cargo run` alone rebuilds the server, not the wasm.
- **In release, the binary is the truth.** `trunk build --release` writes
  `dist/`, and the server has to be *rebuilt* after it — otherwise the binary
  still carries whatever `dist/` held when it was last compiled. Bundles first,
  then `cargo build -p rn-site`.

`tokens.css` follows the same rule and is embedded from this directory rather
than from a `dist/`, so an edit to it reaches the askama pages after a server
rebuild and the bundles' fingerprinted copies after a trunk build.
