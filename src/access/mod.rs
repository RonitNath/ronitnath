//! Visibility policy computation. Persistence loads policy inputs; this module is pure.

pub mod level;

pub use level::{
    AudiencePolicy, CircleGrant, Level, OverrideKind, PersonOverride, level_for,
    level_for_direct_hit,
};
