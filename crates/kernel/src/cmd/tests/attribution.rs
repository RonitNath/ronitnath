//! Attribution: which party a command records, and which row its event names.
//!
//! `ActAs` moves attribution and leaves every authorisation where it was, so
//! these two facts are separate from the ones `authority.rs` tells and from
//! each other. `audit.acting_as` is the party the session speaks as; the
//! event's own subject is the row the command moved. Both were, at different
//! times, the person by accident.

use super::world::{self, key, public};
use crate::bind;
use crate::cmd;
use crate::event::Event;
use crate::ids::Id;
use crate::testing::{Local, TEST_EPOCH};

/// Every command that takes only "my own" arguments still records the party
/// the session speaks as.
///
/// `ActAs` moves attribution and nothing else, and the six commands here are
/// the ones that read no `refs::` argument at all — so each of them reached
/// for [`crate::cmd::member`]'s middle value, which is the *person*, and wrote
/// that into `audit.acting_as`. An organization's history then read as the
/// human's, which is exactly the fact `ActAs` exists to record. The
/// authorisation half is asserted here too, in the same test: `disable` on the
/// organization is allowed because the *person* owns it, not because the
/// session points at it.
#[tokio::test]
async fn the_k1_commands_attribute_themselves_to_the_party_the_session_speaks_as() {
    use rn_api::commands::{
        ActAs, AddFactor, Disable, Enable, FactorKind, RemoveFactor, RevokeSession, SignOut,
    };

    let harness = Local::new();
    let founder = world::person(&harness, "Founder", "attr-founder@example.test").await;
    let (org, _) = world::organization(&harness, &founder, "Isoastra")
        .await
        .expect("founds");
    let organization: Id<crate::ids::Person> = Id::new(org.get());

    cmd::act_as(
        &harness.ctx(founder.principal.clone()),
        &ActAs {
            party: Some(public(key(&harness), org)),
        },
    )
    .await
    .expect("its admin may speak as it");

    // A second session of the same identity, for `revoke-session` to end.
    let other = harness
        .sign_in("attr-founder@example.test")
        .await
        .expect("signs in again");
    let doomed = other.principal.session().expect("a member has a session");

    // What the next request resolves to: the same identity and the same
    // person, pointed at the organization.
    let acting = crate::principal::Principal::Member {
        identity: founder.principal.identity().expect("has one"),
        person: Some(founder.person()),
        acting_as: organization,
        session: founder.principal.session().expect("has one"),
        impersonated_by: None,
    };

    cmd::add_factor(
        &harness.ctx(acting.clone()),
        &AddFactor {
            kind: FactorKind::Email,
            value: "attr-second@example.test".into(),
            identity: None,
        },
    )
    .await
    .expect("adds an address");
    let added = crate::ids::Id::<crate::ids::Factor>::new(
        world::count(
            &harness,
            "SELECT max(id) AS n FROM factor WHERE value = $1",
            bind!["attr-second@example.test"],
        )
        .await,
    );
    cmd::remove_factor(
        &harness.ctx(acting.clone()),
        &RemoveFactor {
            factor: public(key(&harness), added),
            identity: None,
        },
    )
    .await
    .expect("takes it off again");
    cmd::revoke_session(
        &harness.ctx(acting.clone()),
        &RevokeSession {
            session: public(key(&harness), doomed),
        },
    )
    .await
    .expect("their own other device");
    // The person owns the organization, so the person may disable it — and
    // the session pointing at it is not what says so.
    let party = public(key(&harness), org);
    cmd::disable(
        &harness.ctx(acting.clone()),
        &Disable {
            party: party.clone(),
            reason: "a drill".into(),
        },
    )
    .await
    .expect("its owner may");
    cmd::enable(&harness.ctx(acting.clone()), &Enable { party })
        .await
        .expect("and may put it back");
    cmd::sign_out(&harness.ctx(acting), &SignOut {})
        .await
        .expect("ends the acting session");

    for command in [
        "add-factor",
        "remove-factor",
        "revoke-session",
        "disable",
        "enable",
        "sign-out",
    ] {
        let attributed = world::count(
            &harness,
            "SELECT count(*) AS n FROM audit WHERE command = $1 AND acting_as = $2",
            bind![command, organization],
        )
        .await;
        assert_eq!(
            attributed, 1,
            "{command} was not attributed to the organization the session speaks as"
        );
    }
}

/// The event names the row the command moved, not the party it is attributed
/// to.
///
/// `audit.acting_as` and the event's own subject are two different facts, and
/// three commands were binding one parameter for both. They agree until
/// somebody runs `ActAs`, after which the feed said an organization had
/// founded, joined and left things its *member* had — and a subscription that
/// keys a diff off `Event::Left { party }` re-read the wrong row.
#[tokio::test]
async fn an_event_names_the_party_it_moved_and_not_the_one_it_is_attributed_to() {
    use rn_api::commands::{ActAs, ClaimLink, CreateOrganization, Leave};
    use rn_api::whoami::MemberRole as WireRole;

    let harness = Local::new();
    let founder = world::person(&harness, "Founder", "subj-founder@example.test").await;
    let host = world::person(&harness, "Host", "subj-host@example.test").await;
    let (org, _) = world::organization(&harness, &founder, "Isoastra")
        .await
        .expect("founds");

    cmd::act_as(
        &harness.ctx(founder.principal.clone()),
        &ActAs {
            party: Some(public(key(&harness), org)),
        },
    )
    .await
    .expect("its admin may speak as it");
    let acting = crate::principal::Principal::Member {
        identity: founder.principal.identity().expect("has one"),
        person: Some(founder.person()),
        acting_as: Id::new(org.get()),
        session: founder.principal.session().expect("has one"),
        impersonated_by: None,
    };

    // Founding: the resource is the person's, so the event must say so.
    let committed = cmd::create_organization(
        &harness.ctx(acting.clone()),
        &CreateOrganization {
            display_name: "Second".into(),
        },
    )
    .await
    .expect("founds a second");
    let Event::OrganizationCreated { owner, .. } = committed.event else {
        panic!("create-organization produced {:?}", committed.event)
    };
    assert_eq!(owner, founder.person(), "the event named the acting party");

    // Joining and leaving: the membership is the person's.
    let team = world::group(&harness, &host, "Team", None)
        .await
        .expect("creates");
    let token = world::invite(
        &harness,
        &host,
        public(key(&harness), team),
        WireRole::Member,
        TEST_EPOCH + 3_600,
    )
    .await
    .expect("mints");
    let committed = cmd::claim_link(
        &harness.ctx(acting.clone()),
        &ClaimLink {
            token: token.expose().to_owned(),
        },
    )
    .await
    .expect("claims");
    let Event::LinkClaimed { party, .. } = committed.event else {
        panic!("claim-link produced {:?}", committed.event)
    };
    assert_eq!(party, founder.person(), "the event named the acting party");

    let committed = cmd::leave(
        &harness.ctx(acting),
        &Leave {
            group: public(key(&harness), team),
        },
    )
    .await
    .expect("leaves");
    let Event::Left { party, .. } = committed.event else {
        panic!("leave produced {:?}", committed.event)
    };
    assert_eq!(party, founder.person(), "the event named the acting party");
}
