# Rebuild gate manifest

| Gate | Command/evidence | Result |
| --- | --- | --- |
| Generator provenance | `web_template@48c1517`, `cargo xtask new ronitnath --example public-site --output ...` | recorded in history |
| Public route allowlist | `/`, `/calendar`, `/livez`, `/healthz`; operator `/readyz`, `/metrics` only | Rust and browser checks |
| Frontend | `pnpm run check`, `pnpm run test`, `pnpm run build` | required |
| Rust | `cargo test --all-features --locked`, `cargo clippy --all-targets --all-features --locked -- -D warnings` | required |
| Visual | live/local screenshots at desktop and mobile, opened and reviewed | evidence paths reported at handoff |
| Dynamic data/admin | absent from `public-site`; documented migration plan | `docs/dynamic-migration.md` |
