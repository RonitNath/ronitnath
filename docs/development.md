# Development

## Prerequisites

- **Rust toolchain:** pinned to `1.94.1` in `rust-toolchain.toml`; `rustup`
  fetches it automatically on first `cargo` invocation.
- **Bun:** used for the UI build and package management. `curl -fsSL
  https://bun.sh/install | bash` if not installed.
- No database, no services — this project has no external dependencies at
  runtime other than the network listener.

## First run

```sh
cd ui && bun install && bun run build && cd ..
cargo run
```

The Rust binary reads `ui/dist/.vite/manifest.json` at startup to resolve
hashed asset URLs. If the manifest is missing, the page still renders but
`<script>` / `<link>` tags will be absent — rebuild the UI.

Default bind is `0.0.0.0:8080`. Override per-run:

```sh
HOST=127.0.0.1 PORT=3000 cargo run
```

`RONITNATH_DEV=1` switches the asset manifest to fall through to
`/ui-dev/<entry>.js` if the entry isn't in the manifest — useful once a
vite dev-server flow is wired.

For local admin access, visit `/enter`. Debug builds automatically route this
through a dev-admin cookie when the Isoastra client secret is still the
placeholder `dev-only-change-me`; release builds keep auth disabled and return
`503` until real SSO credentials are configured.

## Edit loop

**UI changes** (`ui/src/**`, `ui/entries/**`):

```sh
cd ui && bun run build
```

The Rust server re-reads the manifest only at startup, so restart `cargo
run` after a UI build to pick up new hashes.

**Template changes** (`templates/**`):

Askama is `build.rs`-driven; `cargo run` picks up template edits on the
next build automatically.

**Rust changes** (`src/**`):

```sh
cargo run
```

## Verification

Before committing:

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cd ui && bun run check   # tsc --noEmit, strict
cd ui && bun run build
```

## Adding an island

1. Create `ui/src/islands/<name>.tsx`. Export a `Component<IslandProps>`.
2. Register it in `ui/src/islands/registry.ts`:
   ```ts
   export const islands: IslandRegistry = {
     "mode-toggle": ModeToggle,
     "<name>": YourComponent,
   };
   ```
3. Emit a container from `templates/*.html`:
   ```html
   <div data-island="<name>" data-bootstrap='{"k":"v"}'></div>
   ```
   `data-bootstrap` is parsed as JSON and passed as `props.bootstrap`.
4. `bun run build` and restart.

## Adding a page

1. Add an askama template under `templates/` extending `_base.html`.
2. Define a `Template` struct + `render_<page>` helper in `src/render.rs`.
3. Route it in `src/routes.rs`.

If multiple routes end up sharing nav/content, factor shared text into
`src/content.rs` (not present yet — single page today).

## Tweaking the starfield

Three CSS layers define the starfield in `ui/src/styles/global.css.ts`:

- `.stars-dim` — densest, fainter
- `.stars-med` — medium density, slow twinkle
- `.stars-bright` — sparse, fast twinkle

Each layer is a large `background-image` composed of many
`radial-gradient(...)` dots with fixed `background-size` tiling. To change
density, edit the gradient count; to change color, adjust the color stops
under each `[data-theme="dark"]` / `[data-theme="light"]` variant.
