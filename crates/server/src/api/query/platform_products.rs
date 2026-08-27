//! `platform-products` — every product this build carries, and what this
//! deployment has decided about each.
//!
//! The only query on this surface whose result set is not a table. The rows
//! are the *catalogue* — compiled in, so their number is a property of the
//! binary rather than of the deployment — and the `product` table supplies one
//! column of each: whether somebody has turned it on, and who and when. A
//! product nobody has ever decided about has no row and reads as off, which is
//! the model's default-deny stated where a reader can see it.
//!
//! Joining in Rust rather than in SQL is therefore not a shortcut: one side of
//! the join is not in the database. It also means a slug the table carries and
//! this build does not simply does not appear — that is a node running an
//! older release than the one that enabled it, and a screen inventing a row
//! for a product it knows nothing about would be a screen stating routes it
//! cannot name.
//!
//! The routes column comes from the catalogue for the same reason it exists at
//! all: turning something off has to say what it is about to take away, and
//! the only honest source for that is the list the router is actually built
//! from (`server::product::routes`).

use rn_kernel::product::{self, Product};
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::json;

use super::{Params, Row};

/// One row per product anybody has decided about — at most one per catalogue
/// entry, so the whole table is a page.
///
/// `changed_by` is a party, resolved to its display name the way every other
/// platform list resolves one: the id is not rendered, because what an
/// operator reads here is a name and the row it belongs to is one drill-in
/// away on `/platform/parties`.
pub(super) const PRODUCTS: &str = "SELECT p.slug, p.enabled, p.changed_at, \
                                   b.display_name AS changed_display \
                            FROM product p \
                            LEFT JOIN party b ON b.id = p.changed_by";

struct Decided {
    slug: String,
    enabled: bool,
    changed_at: Timestamp,
    changed_display: Option<String>,
}

impl FromRow for Decided {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            slug: row.text("slug")?,
            enabled: row.int("enabled")? != 0,
            changed_at: row.int("changed_at")?,
            changed_display: row.text_opt("changed_display")?,
        })
    }
}

pub(super) async fn list(reads: &impl Reads, _params: &Params) -> Outcome<Vec<Row>> {
    let decided = reads.query::<Decided>(PRODUCTS, Vec::new()).await?;
    Ok(product::CATALOGUE
        .iter()
        .map(|product| {
            let row = decided.iter().find(|row| row.slug == product.slug);
            catalogue_row(product, row)
        })
        .collect())
}

/// One product, as the screen renders it.
fn catalogue_row(product: &'static Product, decided: Option<&Decided>) -> Row {
    let enabled = decided.is_some_and(|row| row.enabled);
    Row {
        // The slug: the row's key in this result set and the argument the
        // toggle sends back. There is no public id, because a product is an
        // entry in the binary rather than a row somebody created.
        key: product.slug.to_owned(),
        value: json!({
            "slug": product.slug,
            "display": product.display,
            "summary": product.summary,
            "enabled": enabled,
            // A word, so the table renders a state rather than a boolean.
            "state": if enabled { "on" } else { "off" },
            // Zero is not a time, and a product nobody has decided about has
            // no time — the column renders it as such rather than as 1970.
            "changed_at": decided.map_or(0, |row| row.changed_at),
            "changed_by": decided.and_then(|row| row.changed_display.clone()),
            "mounts": product.mounts,
        }),
    }
}
