//! Turning a product on and off, at runtime, on every node.
//!
//! One field each, and it is a slug rather than an id: a product is not a row
//! that was created, it is an entry in the binary's own catalogue
//! (`rn_kernel::product::CATALOGUE`), and the row the command writes is only
//! the deployment's decision about it. So there is nothing to encrypt and
//! nothing to decode — a slug this build does not carry is refused by the
//! kernel with the uniform decline, and no row is written.
//!
//! There is no `reason`, unlike the operator's own four. A delegation of
//! authority has no second actor to check it and is therefore only reviewable
//! if somebody said why at the time; enabling a product is reviewable from
//! what it did — the routes appeared, the audit row names who and when, and
//! the deployment's readers can see the result. Asking for prose here would
//! be ceremony that trains people to type "ok".

use serde::{Deserialize, Serialize};

/// Turn a product on: its routes start answering, on every node, without a
/// restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnableProduct {
    /// The catalogue slug.
    pub slug: String,
}

/// Turn a product off: its routes answer `404` — the same answer a product
/// this deployment never had gets — and everything it wrote stays exactly
/// where it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisableProduct {
    /// The catalogue slug.
    pub slug: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::round_trip;

    #[test]
    fn product_commands_round_trip() {
        round_trip(&EnableProduct {
            slug: "starscape".into(),
        });
        round_trip(&DisableProduct {
            slug: "starscape".into(),
        });
    }
}
