//! Process-level wiring: config, secrets, database boot, telemetry, graceful shutdown.

pub mod config;
pub mod db;
pub mod shutdown;
pub mod telemetry;
