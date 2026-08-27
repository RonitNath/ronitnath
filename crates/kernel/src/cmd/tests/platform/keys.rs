//! B6.2 — retiring a signing key, and refusing to while tokens under it live.

use super::super::prelude::*;
use super::super::world;
use crate::cmd;

#[tokio::test]
async fn a_key_is_not_retired_while_tokens_under_it_are_alive_unless_it_is_forced() {
    use rn_api::commands::{RegisterClient, RetireKey, RotateSigningKey};
    use rn_api::oidc::{ClientAuthMethod, ClientMetadata, GrantType, Scope};

    let harness = Local::new();
    let operator = world::person(&harness, "Op", "key-op@example.test").await;
    make_operator(&harness, operator.person()).await;
    let ctx = || harness.ctx(operator.principal.clone());

    cmd::register_client(
        &ctx(),
        &RegisterClient {
            owner: None,
            metadata: ClientMetadata {
                client_name: "A relying party".into(),
                client_uri: None,
                logo_uri: None,
                redirect_uris: vec!["https://rp.example.test/callback".into()],
                post_logout_redirect_uris: Vec::new(),
                backchannel_logout_uri: None,
                token_endpoint_auth_method: ClientAuthMethod::ClientSecretBasic,
                jwks: None,
                grant_types: vec![GrantType::AuthorizationCode],
                scopes: vec![Scope::Openid],
                members_only: false,
                trusted: false,
            },
        },
    )
    .await
    .expect("an operator registers one");

    // Two rotations: the first key is `retiring`, the second is `active`.
    cmd::rotate_signing_key(&ctx(), &RotateSigningKey {})
        .await
        .expect("mints the first");
    let first = crate::oidc::key::published(harness.store())
        .await
        .expect("reads")
        .into_iter()
        .next()
        .expect("one key")
        .kid;
    harness.advance(60);
    cmd::rotate_signing_key(&ctx(), &RotateSigningKey {})
        .await
        .expect("mints the second");

    // A live token issued while the first key was active. Inserted rather than
    // granted: the flow that mints one is the OpenID Provider's and lives in
    // the server's suites, and what this test is about is the *count* the
    // decision rests on — a service token, so the schema's two CHECKs hold
    // without a session to hang it off.
    harness
        .store()
        .execute(
            "INSERT INTO oidc_token \
             (token_hash, kind, client_id, service_party_id, scopes, expires_at, created_at) \
             SELECT $1, 'access', c.id, c.service_party_id, 'openid', $2, $3 \
             FROM oidc_client c LIMIT 1",
            bind![
                vec![7u8; 32],
                crate::testing::TEST_EPOCH + 3_600,
                crate::testing::TEST_EPOCH + 1
            ],
        )
        .await
        .expect("a token exists");

    let retire = RetireKey {
        kid: first.clone(),
        force: false,
        reason: "the overlap is over".into(),
    };
    let refused = cmd::retire_key(&ctx(), &retire).await;
    assert!(
        matches!(refused, Err(ref e) if e.is_decline()),
        "retiring it is what makes that token unverifiable"
    );
    assert!(
        crate::oidc::key::published(harness.store())
            .await
            .expect("reads")
            .iter()
            .any(|row| row.kid == first),
        "and it is still published"
    );

    // Forced, with a reason, it goes — and the reason and the number it
    // overrode are both on the audit row, because that is the sentence
    // somebody will look for afterwards.
    let forced = RetireKey {
        kid: first.clone(),
        force: true,
        reason: "the key is believed compromised".into(),
    };
    cmd::retire_key(&ctx(), &forced)
        .await
        .expect("a forced retire is a decision, and it is allowed");
    let recorded = count(
        &harness,
        "SELECT count(*) AS n FROM audit WHERE command = 'retire-key' \
           AND json_extract(payload, '$.forced') = 1 \
           AND json_extract(payload, '$.alive') = 1 \
           AND json_extract(payload, '$.reason') = 'the key is believed compromised'",
    )
    .await;
    assert_eq!(recorded, 1, "the decision, the number and the reason");

    // The JWKS no longer carries it, which is what retiring *is*.
    assert!(
        !crate::oidc::key::published(harness.store())
            .await
            .expect("reads")
            .iter()
            .any(|row| row.kid == first),
        "a retired key is out of the published document"
    );

    // A second retire of the same kid declines: `retiring` is the only status
    // this command acts on, and so is the active key, even forced.
    assert!(matches!(cmd::retire_key(&ctx(), &forced).await, Err(ref e) if e.is_decline()));
    let active = crate::oidc::key::published(harness.store())
        .await
        .expect("reads")[0]
        .kid
        .clone();
    let refused = cmd::retire_key(
        &ctx(),
        &RetireKey {
            kid: active,
            force: true,
            reason: "even forced".into(),
        },
    )
    .await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));

    // A reason is mandatory, and its absence is an `Invalid` a form can render
    // rather than the uniform decline — the caller knows what it sent.
    let empty = cmd::retire_key(
        &ctx(),
        &RetireKey {
            kid: first,
            force: false,
            reason: "   ".into(),
        },
    )
    .await;
    assert!(matches!(
        empty,
        Err(KernelError::Invalid(Invalid::Missing("reason")))
    ));
}
