# stage_1

Base template project — fork this to start a new app.

It is a small [Axum](https://github.com/tokio-rs/axum) web server that renders
[Askama](https://github.com/askama-rs/askama) HTML templates and serves static
assets. The structure is intentionally split into small modules so that, as the
app grows, code lands in the right place instead of piling up in `main.rs` or
other central files.

## Layout

    src/
      main.rs        Entry point — initializes telemetry and starts the server.
      app.rs         Router assembly and server bootstrap.
      config.rs      Environment-sourced runtime configuration.
      state.rs       Shared application state (AppState).
      telemetry.rs   Tracing / logging setup.
      view.rs        Template-rendering helper.
      handlers/      HTTP handlers, one module per feature area.
        mod.rs
        home.rs      Home page.
        errors.rs    Fallback / error responses.
    templates/       Askama HTML templates (`_layout.html` is the base).
    static/
      css/           Stylesheets, split by concern.
      js/            Client-side scripts.

Add a new handler module per feature area and a matching CSS/JS file rather than
growing any single file.

## Running

    cargo run

The server binds to `127.0.0.1:3000` by default. Override with `BIND_ADDR`:

    BIND_ADDR=0.0.0.0:8080 cargo run

## Tech stack

- Axum (web framework)
- Tokio (async runtime)
- Askama (HTML templates)
- tower-http (static files, request tracing)
- tracing (structured logging)
