//! C9 — what a disable ends, and what an enable puts back.

use rn_api::commands::Disable;

use super::super::prelude::*;
use super::super::world::{self, key, public};
use super::count_of;
use crate::cmd;

#[tokio::test]
async fn a_disable_puts_the_invitations_to_sleep_and_an_enable_wakes_them() {
    use rn_api::commands::{ClaimLink, Enable};
    use rn_api::whoami::MemberRole as WireRole;

    let harness = Local::new();
    let minter = world::person(&harness, "Minter", "casc-m@example.test").await;
    let joiner = world::person(&harness, "Joiner", "casc-j@example.test").await;
    let operator = world::person(&harness, "Op", "casc-op@example.test").await;
    make_operator(&harness, operator.person()).await;

    let group = world::group(&harness, &minter, "Team", None)
        .await
        .expect("creates");
    let token = world::invite(
        &harness,
        &minter,
        public(key(&harness), group),
        WireRole::Member,
        crate::testing::TEST_EPOCH + 3_600,
    )
    .await
    .expect("mints");
    let claim = ClaimLink {
        token: token.expose().to_owned(),
    };

    // What the confirm control would have said. C9.3: the preview and the
    // record are the same function, so they cannot describe different
    // cascades.
    let preview = crate::cascade::counts(harness.store(), minter.person())
        .await
        .expect("counts");
    assert_eq!(preview.links, 1, "one unclaimed invitation");
    assert_eq!(preview.sessions, 1, "the minter's own");

    cmd::disable(
        &harness.ctx(operator.principal.clone()),
        &Disable {
            party: public(key(&harness), minter.person()),
            reason: "an operator ruling".into(),
        },
    )
    .await
    .expect("an operator disables");

    // Finding F3: before this, the token still worked. The refusal is the
    // uniform decline — a claimant learns that the token does not work now and
    // nothing about why.
    let refused = cmd::claim_link(&harness.ctx(joiner.principal.clone()), &claim).await;
    assert!(
        matches!(refused, Err(ref e) if e.is_decline()),
        "a disabled person's outstanding invitations stop working"
    );
    // The row is still there. `RevokeLink` means gone for good; this does not.
    assert_eq!(
        count(&harness, "SELECT count(*) AS n FROM link").await,
        1,
        "suspended, not deleted"
    );

    // The audit row carries the counts the preview promised.
    let recorded = count_of(
        &harness,
        "SELECT json_extract(payload, '$.counts.links') AS n FROM audit \
         WHERE command = 'disable'",
        bind![],
    )
    .await;
    assert_eq!(recorded, preview.links);

    cmd::enable(
        &harness.ctx(operator.principal.clone()),
        &Enable {
            party: public(key(&harness), minter.person()),
        },
    )
    .await
    .expect("an operator enables");

    cmd::claim_link(&harness.ctx(joiner.principal.clone()), &claim)
        .await
        .expect("the invitation works again");
}

#[tokio::test]
async fn disabling_a_container_stops_people_joining_it() {
    use rn_api::commands::ClaimLink;
    use rn_api::whoami::MemberRole as WireRole;

    let harness = Local::new();
    let owner = world::person(&harness, "Owner", "cont-o@example.test").await;
    let joiner = world::person(&harness, "Joiner", "cont-j@example.test").await;
    let operator = world::person(&harness, "Op", "cont-op@example.test").await;
    make_operator(&harness, operator.person()).await;

    let (org, _) = world::organization(&harness, &owner, "Isoastra")
        .await
        .expect("founds");
    let token = world::invite(
        &harness,
        &owner,
        public(key(&harness), org),
        WireRole::Member,
        crate::testing::TEST_EPOCH + 3_600,
    )
    .await
    .expect("mints");

    cmd::disable(
        &harness.ctx(operator.principal.clone()),
        &Disable {
            party: public(key(&harness), org),
            reason: "the tenant is gone".into(),
        },
    )
    .await
    .expect("an operator disables an organization");

    let refused = cmd::claim_link(
        &harness.ctx(joiner.principal.clone()),
        &ClaimLink {
            token: token.expose().to_owned(),
        },
    )
    .await;
    assert!(
        matches!(refused, Err(ref e) if e.is_decline()),
        "a seat in a container nobody can use is not one to accept"
    );
}
