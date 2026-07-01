//! HTTP request handlers, one module per logical area of the app.
//!
//! Add a new module here for each feature area instead of growing a single
//! file. Each handler module owns its route handlers and any Askama template
//! structs they render.

pub mod about;
pub mod errors;
pub mod home;
