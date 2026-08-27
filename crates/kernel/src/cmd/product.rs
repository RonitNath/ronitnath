//! `EnableProduct` and `DisableProduct` — a product's routes appear and vanish
//! on every node, without a redeploy.
//!
//! Two commands with one shape, and the shape is deliberately small. Each
//! writes **one row** — the `product` row and its audit row, and nothing else.
//! Disabling in particular touches no product data: the story is "a disabled
//! product's routes vanish and its data stays", so a command that swept
//! anything up would be a command that made "off" into a decision nobody could
//! take back.
//!
//! What makes them safe is not what they write but what they refuse.
//!
//! * **A slug this build does not carry is declined**, before anything is
//!   read and with no row written. The catalogue is compiled in
//!   ([`crate::product::CATALOGUE`]) exactly so that a typo is a refusal
//!   rather than a row that quietly enables nothing — the argument
//!   [`crate::relation::vocabulary`] makes about unregistered kinds.
//! * **Operator only**, through [`Want::Platform`], which is the clause that
//!   is nobody else's at all.
//! * **Sensitive** ([`authority::SENSITIVE`]): turning the deployment's
//!   surface on or off from an unattended screen is precisely what the
//!   fifteen-minute window exists to stop.
//!
//! Enabling something already enabled writes nothing and therefore declines,
//! the same way granting a relation somebody already holds does
//! ([`super::operator`]): the guard in the statement is the transactional
//! statement of "this is a change", and a command that changed nothing has no
//! event to put on the feed.

use rn_api::commands::{DisableProduct, EnableProduct};

use super::{Applied, Batch, Ctx, refs, run};
use crate::audit;
use crate::authority::{self, Want};
use crate::bind;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::product;
use crate::store::Sql;

/// The decision, written once per product.
///
/// `ON CONFLICT … WHERE product.enabled <> $2` is the guard: a toggle that
/// would not change anything affects no row, so the audit statement beside it
/// writes nothing either and [`run`] answers the uniform decline rather than
/// putting an event on the feed that says something happened.
const SET: &str = "INSERT INTO product (slug, enabled, changed_at, changed_by) \
     VALUES ($1, $2, $3, $4) \
     ON CONFLICT (slug) DO UPDATE SET enabled = $2, changed_at = $3, changed_by = $4 \
     WHERE product.enabled <> $2";

const ENABLE_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'enable-product', $2, $3, $4, $5, \
            json_object('event', 'enable-product', 'slug', $6) \
     WHERE changes() > 0";

const DISABLE_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'disable-product', $2, $3, $4, $5, \
            json_object('event', 'disable-product', 'slug', $6) \
     WHERE changes() > 0";

/// Turn a product on.
pub async fn enable_product<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &EnableProduct,
) -> Outcome<Committed> {
    toggle(ctx, &args.slug, true, args).await
}

/// Turn a product off. Nothing it wrote is touched.
pub async fn disable_product<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &DisableProduct,
) -> Outcome<Committed> {
    toggle(ctx, &args.slug, false, args).await
}

/// The one body both commands are.
///
/// They are written as one because the difference between them is a bit and a
/// word in a payload, and two copies of an authorisation is two places for one
/// of them to stop matching.
async fn toggle<S: Sql, F: Feed, A: serde::Serialize + Sync>(
    ctx: &Ctx<'_, S, F>,
    slug: &str,
    on: bool,
    args: &A,
) -> Outcome<Committed> {
    let (identity, _, acting_as) = refs::actor(&ctx.principal)?;
    // Before anything is read: a slug that is not in this binary names nothing
    // this deployment could serve, so there is nothing to decide about it.
    let Some(product) = product::find(slug) else {
        return decline();
    };
    authority::require(ctx, Want::Platform).await?;
    authority::fresh_enough(ctx).await?;

    let now = ctx.now();
    let sql = if on { ENABLE_AUDIT } else { DISABLE_AUDIT };
    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(SET, bind![product.slug, on, now, acting_as]);
        batch.one(
            sql,
            bind![
                ctx.key.to_string(),
                identity,
                acting_as,
                now,
                audit::digest_of(args),
                product.slug
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(committed)
}

#[cfg(test)]
mod tests {
    use rn_api::commands::{DisableProduct, EnableProduct};
    use uuid::Uuid;

    use crate::cmd::{disable_product, enable_product};
    use crate::event::Event;
    use crate::ids::{Id, Person};
    use crate::product;
    use crate::store::{Count, Reads};
    use crate::testing::Local;
    use crate::{Principal, bind};

    /// The catalogue's first entry, which is what a test toggles.
    fn a_product() -> String {
        product::CATALOGUE[0].slug.to_owned()
    }

    async fn make_operator(harness: &Local, person: Id<Person>) {
        harness
            .store()
            .execute(
                "INSERT INTO relation \
                 (object_kind, object_id, relation, subject_kind, subject_id, at) \
                 VALUES ('platform', 0, 'operator', 'person', $1, $2)",
                bind![person, crate::testing::TEST_EPOCH],
            )
            .await
            .expect("an operator relation");
    }

    async fn count(harness: &Local, sql: &'static str) -> i64 {
        harness
            .store()
            .query::<Count>(sql, bind![])
            .await
            .expect("query runs")[0]
            .0
    }

    /// An operator, and the principal their session resolves to.
    async fn operator(harness: &Local) -> Principal {
        let who = harness
            .register("Operator", "products-operator@example.test")
            .await
            .expect("registers");
        make_operator(
            harness,
            who.principal.acting_as().expect("acts as somebody"),
        )
        .await;
        who.principal
    }

    #[tokio::test]
    async fn a_slug_this_build_does_not_carry_is_declined_and_writes_no_row() {
        let harness = Local::new();
        let principal = operator(&harness).await;
        let refused = enable_product(
            &harness.ctx(principal),
            &EnableProduct {
                slug: "events".into(),
            },
        )
        .await;
        assert!(matches!(refused, Err(ref e) if e.is_decline()));
        assert_eq!(
            count(&harness, "SELECT count(*) AS n FROM product").await,
            0
        );
        assert_eq!(
            count(
                &harness,
                "SELECT count(*) AS n FROM audit WHERE command = 'enable-product'"
            )
            .await,
            0
        );
    }

    #[tokio::test]
    async fn a_member_who_is_not_an_operator_cannot_toggle_anything() {
        let harness = Local::new();
        let who = harness
            .register("Member", "products-member@example.test")
            .await
            .expect("registers");
        let refused = enable_product(
            &harness.ctx(who.principal),
            &EnableProduct { slug: a_product() },
        )
        .await;
        assert!(matches!(refused, Err(ref e) if e.is_decline()));
        assert_eq!(
            count(&harness, "SELECT count(*) AS n FROM product").await,
            0
        );
    }

    #[tokio::test]
    async fn enable_disable_enable_is_three_audit_rows_and_one_product_row() {
        let harness = Local::new();
        let principal = operator(&harness).await;
        let slug = a_product();

        let committed = enable_product(
            &harness.ctx(principal.clone()),
            &EnableProduct { slug: slug.clone() },
        )
        .await
        .expect("enables");
        assert!(
            matches!(committed.event, Event::ProductEnabled { slug: ref s } if *s == slug),
            "{:?}",
            committed.event
        );

        let committed = disable_product(
            &harness.ctx(principal.clone()),
            &DisableProduct { slug: slug.clone() },
        )
        .await
        .expect("disables");
        assert!(matches!(committed.event, Event::ProductDisabled { .. }));

        enable_product(
            &harness.ctx(principal),
            &EnableProduct { slug: slug.clone() },
        )
        .await
        .expect("enables again");

        // One row per product, however many times it has been decided about.
        assert_eq!(
            count(&harness, "SELECT count(*) AS n FROM product").await,
            1
        );
        assert_eq!(
            count(
                &harness,
                "SELECT count(*) AS n FROM product WHERE enabled = 1"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &harness,
                "SELECT count(*) AS n FROM audit \
                 WHERE command IN ('enable-product', 'disable-product')"
            )
            .await,
            3
        );
        // And nothing the product owns was touched by the disable: the
        // deployment's own rows are all still there.
        assert_eq!(
            count(
                &harness,
                "SELECT count(*) AS n FROM party WHERE kind = 'person'"
            )
            .await,
            1
        );
    }

    #[tokio::test]
    async fn turning_on_what_is_already_on_changes_nothing_and_says_so() {
        let harness = Local::new();
        let principal = operator(&harness).await;
        let slug = a_product();
        enable_product(
            &harness.ctx(principal.clone()),
            &EnableProduct { slug: slug.clone() },
        )
        .await
        .expect("enables");

        let again = enable_product(
            &harness.ctx(principal),
            &EnableProduct { slug: slug.clone() },
        )
        .await;
        assert!(matches!(again, Err(ref e) if e.is_decline()));
        assert_eq!(
            count(
                &harness,
                "SELECT count(*) AS n FROM audit WHERE command = 'enable-product'"
            )
            .await,
            1
        );
    }

    #[tokio::test]
    async fn the_projection_reads_back_what_the_command_wrote() {
        let harness = Local::new();
        let principal = operator(&harness).await;
        let slug = a_product();

        assert_eq!(
            product::read(&harness.store().reads())
                .await
                .expect("reads"),
            0
        );
        enable_product(
            &harness.ctx(principal.clone()),
            &EnableProduct { slug: slug.clone() },
        )
        .await
        .expect("enables");

        let bits = product::read(&harness.store().reads())
            .await
            .expect("reads");
        let set = product::ProductSet::from_bits(bits);
        assert!(set.holds(&slug));

        disable_product(
            &harness.ctx(principal),
            &DisableProduct { slug: slug.clone() },
        )
        .await
        .expect("disables");
        let bits = product::read(&harness.store().reads())
            .await
            .expect("reads");
        assert!(!product::ProductSet::from_bits(bits).holds(&slug));
    }

    #[tokio::test]
    async fn an_unfresh_session_is_asked_for_its_password_rather_than_declined() {
        let harness = Local::new();
        let principal = operator(&harness).await;
        // Past the re-authentication window: a sensitive command is refused in
        // the one way a caller can act on.
        harness.advance(crate::authority::PLATFORM_REAUTH_WINDOW + 1);
        let refused = enable_product(
            &harness.ctx_keyed(principal, Uuid::new_v4()),
            &EnableProduct { slug: a_product() },
        )
        .await;
        assert!(matches!(
            refused,
            Err(crate::error::KernelError::ReAuthRequired)
        ));
        assert_eq!(
            count(&harness, "SELECT count(*) AS n FROM product").await,
            0
        );
    }
}
