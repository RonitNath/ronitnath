//! Resource grants: ownership, sharing, and revocation against a live node.
//!
//! The two-layer rule is what these tests protect: account membership answers
//! "what can you do in this account", grants answer "what can you do to this
//! thing", and neither implies the other. Ownership needs no row; a share is
//! exactly one row; a revoked share behaves as if it never existed.

mod common;

use axum::http::StatusCode;
use common::{TEST_PASSWORD, TestApp};
use rn_site::auth::{
    Access, GrantSubject, ResourceAction, ResourceKind, SessionContext, ShareRole, grants, store,
};

/// Register an identity and return a resolved [`SessionContext`] for it — the
/// same shape the guard would attach, produced through the real sign-in path.
async fn session_for(app: &TestApp, email: &str) -> (store::Registration, SessionContext) {
    app.server
        .post("/api/auth/register")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await
        .assert_status(StatusCode::CREATED);
    let registration = store::registration_for_email(&app.db, email)
        .await
        .expect("lookup")
        .expect("just registered");
    let token = store::create_session(&app.db, &registration, None)
        .await
        .expect("session");
    let context = store::resolve_session(&app.db, &token)
        .await
        .expect("resolve")
        .expect("live session");
    (registration, context)
}

/// A synthetic document public id: the resource table does not exist yet, and
/// nothing in the grants layer needs it to — grants name resources by
/// (kind, public id) precisely so they do not depend on the resource's schema.
const DOC: &str = "0d9a1c2e-7b7e-4a8f-9f3a-1234567890ab";

#[tokio::test]
async fn owner_needs_no_grant_and_strangers_see_nothing() {
    let app = TestApp::boot().await;
    let (_owner_reg, owner) = session_for(&app, "owner@example.test").await;
    let (_stranger_reg, stranger) = session_for(&app, "stranger@example.test").await;

    let owner_account = owner.account_id;

    let access =
        grants::resolve_access(&app.db, ResourceKind::Document, DOC, owner_account, &owner)
            .await
            .expect("resolve");
    assert_eq!(
        access,
        Some(Access::Owner),
        "ownership is structural, not a row"
    );

    let access = grants::resolve_access(
        &app.db,
        ResourceKind::Document,
        DOC,
        owner_account,
        &stranger,
    )
    .await
    .expect("resolve");
    assert_eq!(
        access, None,
        "no grant means the resource does not exist for this session"
    );

    app.shutdown().await;
}

#[tokio::test]
async fn identity_share_grants_exactly_the_role_and_revoke_removes_it() {
    let app = TestApp::boot().await;
    let (owner_reg, owner) = session_for(&app, "owner@example.test").await;
    let (_reader_reg, reader) = session_for(&app, "reader@example.test").await;

    grants::grant(
        &app.db,
        ResourceKind::Document,
        DOC,
        GrantSubject::Identity(reader.identity_id),
        ShareRole::Viewer,
        owner_reg.identity_id,
    )
    .await
    .expect("grant viewer");

    let access = grants::resolve_access(
        &app.db,
        ResourceKind::Document,
        DOC,
        owner.account_id,
        &reader,
    )
    .await
    .expect("resolve")
    .expect("shared");
    assert_eq!(access, Access::Shared(ShareRole::Viewer));
    assert!(access.allows(ResourceAction::Read));
    assert!(!access.allows(ResourceAction::Comment));
    assert!(!access.allows(ResourceAction::Write));

    // Re-granting replaces the role in place — no revoke-then-grant gap.
    grants::grant(
        &app.db,
        ResourceKind::Document,
        DOC,
        GrantSubject::Identity(reader.identity_id),
        ShareRole::Editor,
        owner_reg.identity_id,
    )
    .await
    .expect("upgrade to editor");
    let access = grants::resolve_access(
        &app.db,
        ResourceKind::Document,
        DOC,
        owner.account_id,
        &reader,
    )
    .await
    .expect("resolve")
    .expect("still shared");
    assert_eq!(access, Access::Shared(ShareRole::Editor));
    assert!(access.allows(ResourceAction::Write));

    // Revoke: idempotent, and afterwards the resource vanishes again.
    assert!(
        grants::revoke(
            &app.db,
            ResourceKind::Document,
            DOC,
            GrantSubject::Identity(reader.identity_id),
        )
        .await
        .expect("revoke")
    );
    assert!(
        !grants::revoke(
            &app.db,
            ResourceKind::Document,
            DOC,
            GrantSubject::Identity(reader.identity_id),
        )
        .await
        .expect("second revoke reports absence, not an error")
    );
    let access = grants::resolve_access(
        &app.db,
        ResourceKind::Document,
        DOC,
        owner.account_id,
        &reader,
    )
    .await
    .expect("resolve");
    assert_eq!(access, None);

    app.shutdown().await;
}

#[tokio::test]
async fn account_share_reaches_the_membership_and_strongest_grant_wins() {
    let app = TestApp::boot().await;
    let (owner_reg, owner) = session_for(&app, "owner@example.test").await;
    let (_member_reg, member) = session_for(&app, "member@example.test").await;

    // Share with the member's whole account…
    grants::grant(
        &app.db,
        ResourceKind::Document,
        DOC,
        GrantSubject::Account(member.account_id),
        ShareRole::Commenter,
        owner_reg.identity_id,
    )
    .await
    .expect("account grant");
    // …and, more weakly, with the member personally. The stronger must win.
    grants::grant(
        &app.db,
        ResourceKind::Document,
        DOC,
        GrantSubject::Identity(member.identity_id),
        ShareRole::Viewer,
        owner_reg.identity_id,
    )
    .await
    .expect("identity grant");

    let access = grants::resolve_access(
        &app.db,
        ResourceKind::Document,
        DOC,
        owner.account_id,
        &member,
    )
    .await
    .expect("resolve")
    .expect("shared twice over");
    assert_eq!(
        access,
        Access::Shared(ShareRole::Commenter),
        "effective access is the strongest matching grant"
    );

    app.shutdown().await;
}
