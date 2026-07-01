//! Logging / tracing setup.

/// Initializes the global tracing subscriber, honoring `RUST_LOG` when set.
pub fn init() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "stage_1=debug,tower_http=info".into()),
        )
        .init();
}
