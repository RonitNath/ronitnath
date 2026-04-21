# ronitnath

Public site for **ronitnath.com** — personal landing page over a starfield
background plus a lightweight `/events` placeholder. The home route renders one
card: name, tagline, and four links (GitHub, Instagram, LinkedIn, Email).

This repo is intentionally minimal. It contains only public content and the
canonical frontend shape. No calendar, no database, no auth.

## Stack

- **Server** — `axum 0.8` + `askama 0.15` (SSR HTML).
- **UI** — `solid-js 1.9` islands built by `vite 7` with `@vanilla-extract`
  generating typed CSS at build time. Package manager: `bun`.
- **Runtime** — Rust `1.94.1` pinned via `rust-toolchain.toml`. Multi-stage
  `Dockerfile` produces a `debian:bookworm-slim` image.

## Request flow

```
GET /
  └─ axum route → render::render_home(&state)
       └─ reads assets::AssetManifest (ui/dist/.vite/manifest.json)
       └─ HomeTemplate { script_src, css_files, … }
       └─ askama renders templates/home.html (extends _base.html)
              <link rel="stylesheet" href="/ui/dist/assets/site-<hash>.css">
              <script type="module" src="/ui/dist/assets/site-<hash>.js">
              <div data-island="mode-toggle"></div>
  └─ browser loads site.ts
       └─ hydrateIslands(registry) mounts <ModeToggle/> into each
          element with a `data-island` attribute matching a registry key.

GET /events
  └─ axum route → render::render_events(&state)
       └─ askama renders templates/events.html (extends _base.html)
```

Vite's manifest plumbs hashed filenames through the server at startup so
the HTML always references the current build artifacts.

## Layout

```
src/
  main.rs         # tokio runtime, config load, router wiring
  config.rs       # host / port / domain from config.toml + env overrides
  assets.rs       # vite manifest parser → entry + css lookup
  render.rs       # HomeTemplate + render helpers
  routes.rs       # axum Router + per-path handlers

templates/
  _base.html      # <html> shell, starfield + nebula layers, inline
                  # theme-cookie hydrator, body block, manifest links/scripts
  home.html       # home-card: name, tagline, 4 social links
  events.html     # simple events placeholder

ui/
  entries/site.ts             # sole vite entry → compiled to ui/dist/assets/
  src/islands/
    hydrate.ts                # scans for [data-island], mounts Solid components
    registry.ts               # { "mode-toggle": ModeToggle }
    mode-toggle.tsx
  src/styles/
    theme.css.ts              # OKLCH dark + light palettes; Agency Bold display
    global.css.ts             # style entrypoint importing split files
    base.css.ts               # font, reset, base document styles
    atmosphere.css.ts         # starfield and nebula background
    layout.css.ts             # nav and main layout
    pages.css.ts              # home and events page styles
    mode-toggle.css.ts

static/
  favicon.png
  fonts/agency-bold.ttf       # served at /static/fonts/agency-bold.ttf
config.toml                   # default host/port/domain (env overrides)
Dockerfile
```

## Conventions

- **Dark and light share one palette model.** Tokens are OKLCH. Light mode
  is activated by `data-theme="light"` on `<html>`. No separate
  `data-named-theme` — selectors are flat `[data-theme=…]`.
- **Theme cookie.** `__rn_theme`. Read by the inline script in
  `_base.html` before paint to prevent FOUC. Written by the `ModeToggle`
  island on click.
- **Atmosphere is CSS-only.** Three stacked starfield divs (`.stars-dim`,
  `.stars-med`, `.stars-bright`) plus the nebula layer are backed by layered
  radial-gradient backgrounds with slow twinkle keyframes. No per-star JS.
- **Font.** `Agency Bold` for the display heading, preloaded from
  `/static/fonts/agency-bold.ttf` in the base template head. Body text
  uses the system UI stack.
- **No calendar, no htmx.** The previous version wired a `/calendar` route
  backed by sqlx. `/events` is currently static public content.

## See also

- `docs/development.md` — running locally and the edit loop
- `docs/deployment.md` — building and shipping the container
