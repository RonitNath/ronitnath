# List available recipes.
default:
    @just --list

# Build the frontend once, then run the server in development mode
# (Ctrl-C to stop). Wires the no-credentials dev login shortcut where the
# selected profile has one — see src/features/dev_login.rs.
dev:
    npx vite build
    cargo run

# Rebuild the frontend on every change; pair with `just backend-watch`.
frontend-watch:
    npx vite build --watch

# Run the server with cargo-watch, rebuilding on Rust changes (requires cargo-watch).
backend-watch:
    cargo watch -x run

# Runs the release-shaped build in production mode against a real,
# operator-configured auth provider (OIDC issuer or local-auth credentials
# store) — for exercising a staging instance, not for everyday development.
# Configuring that provider is out of scope here; APP_ENV must be set to
# "production" in the environment this runs against.
run:
    npx vite build
    cargo build --release
    ./target/release/ronitnath

# Full production-shaped build: frontend bundle + release binary.
build:
    npx vite build
    cargo build --release

# Type-check and lint everything without emitting artifacts.
check:
    tsc --noEmit
    cargo check

# Run the full test suite (frontend + backend).
test:
    npx vitest run
    cargo test

# Remove build artifacts.
clean:
    rm -rf static/public target
