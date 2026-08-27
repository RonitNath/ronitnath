//! What the OP's pieces promise each other, proved against a real database.

use rn_api::oidc::Scope;

use super::*;
use crate::ids::Id;
use crate::store::Reads as _;
use crate::testing::Local;

#[test]
fn a_published_jwks_names_every_key_and_says_what_each_is_for() {
    let rows = vec![
        KeyRow {
            id: Id::new(2),
            kid: "second".to_owned(),
            status: "active".to_owned(),
            modulus: "bW9k".to_owned(),
            exponent: "AQAB".to_owned(),
            created_at: 0,
        },
        KeyRow {
            id: Id::new(1),
            kid: "first".to_owned(),
            status: "retiring".to_owned(),
            modulus: "b2xk".to_owned(),
            exponent: "AQAB".to_owned(),
            created_at: 0,
        },
    ];
    let document = jwks_document(&rows);
    let keys = document["keys"].as_array().expect("an array");
    assert_eq!(keys.len(), 2);
    for key in keys {
        assert_eq!(key["kty"], "RSA");
        assert_eq!(key["use"], "sig");
        assert_eq!(key["alg"], "RS256");
        assert!(key.get("d").is_none(), "never a private half");
    }
    assert_eq!(keys[0]["kid"], "second");
}

#[test]
fn a_client_jwks_verifies_its_own_assertion_and_refuses_a_strangers() {
    let mine = rsa::RsaPrivateKey::new(&mut rand_core::OsRng, 1024).expect("a key");
    let theirs = rsa::RsaPrivateKey::new(&mut rand_core::OsRng, 1024).expect("a key");
    let (n, e) = key::jwk_parts(&rsa::RsaPublicKey::from(&mine));
    let jwks = serde_json::json!({
        "keys": [{"kty": "RSA", "kid": "client-key", "n": n, "e": e}]
    })
    .to_string();

    let claims = serde_json::json!({"iss": "c_x", "jti": "one"});
    let signed = jwt::sign(&mine, "client-key", jwt::TYP_JWT, &claims).expect("it signs");
    assert!(jwks_verifies(&jwks, &jwt::split(&signed).expect("parts")));

    let forged = jwt::sign(&theirs, "client-key", jwt::TYP_JWT, &claims).expect("it signs");
    assert!(!jwks_verifies(&jwks, &jwt::split(&forged).expect("parts")));
    assert!(!jwks_verifies(
        "not json",
        &jwt::split(&signed).expect("parts")
    ));
    assert!(!jwks_verifies(
        r#"{"keys":[]}"#,
        &jwt::split(&signed).expect("parts")
    ));
}

/// A `sub` is not an id, and the two cannot be confused for one another.
#[test]
fn a_subject_is_never_an_id_of_anything() {
    let local = Local::new();
    let key = local.store().ids();
    let sub = subject::mint();
    for id in 1..64i64 {
        assert_ne!(
            sub,
            Id::<crate::ids::Person>::new(id).public(key).to_string()
        );
        assert_ne!(
            sub,
            Id::<crate::ids::Identity>::new(id).public(key).to_string()
        );
    }
    assert!(
        !sub.starts_with("p_") && !sub.starts_with("i_") && !sub.starts_with("c_"),
        "and it does not even look like one"
    );
}

#[test]
fn a_scope_string_round_trips_through_the_column_a_consent_stores_it_in() {
    let asked = vec![Scope::Openid, Scope::Email, Scope::OfflineAccess];
    let stored = Scope::join(&asked);
    let row = ConsentRow {
        client_id: Id::new(1),
        person_id: Id::new(1),
        scopes: stored.clone(),
        at: 0,
    };
    assert_eq!(row.granted(), asked);
    assert!(row.covers(&asked));
    assert_eq!(stored, "openid email offline_access");
}

/// The schema's own promises: a group never owns a client, a user token
/// always has a session, and a token is a person's or a service's.
#[tokio::test]
async fn the_schema_refuses_what_the_model_says_cannot_exist() {
    let local = Local::new();
    let store = local.store();
    store
        .execute(
            "INSERT INTO party (kind, display_name, status, created_at) \
             VALUES ('group', 'a group', 'active', 1)",
            crate::bind![],
        )
        .await
        .expect("a group");
    store
        .execute(
            "INSERT INTO party (kind, display_name, status, created_at) \
             VALUES ('service', 'a service', 'active', 1)",
            crate::bind![],
        )
        .await
        .expect("a service party");

    let register = |owner: i64| {
        store.execute(
            "INSERT INTO oidc_client (owner_party_id, service_party_id, client_name, \
             redirect_uris, post_logout_redirect_uris, token_endpoint_auth_method, \
             grant_types, scopes, trusted, members_only, created_at) \
             VALUES ($1, 2, 'c', '[]', '[]', 'none', '[]', '[]', 0, 0, 1)",
            crate::bind![owner],
        )
    };
    assert!(register(1).await.is_err(), "a group never owns a client");

    store
        .execute(
            "INSERT INTO party (kind, display_name, status, created_at) \
             VALUES ('person', 'somebody', 'active', 1)",
            crate::bind![],
        )
        .await
        .expect("a person");
    register(3).await.expect("a person may own one");

    // A user token with no session: the CHECK refuses it.
    assert!(
        store
            .execute(
                "INSERT INTO oidc_token (token_hash, kind, client_id, person_id, scopes, \
                 expires_at, created_at) VALUES (x'00', 'access', 1, 3, 'openid', 9, 1)",
                crate::bind![],
            )
            .await
            .is_err(),
        "a user token is bound to a session"
    );
    // A token that is nobody's, and one that is everybody's.
    for both in [
        "INSERT INTO oidc_token (token_hash, kind, client_id, scopes, expires_at, created_at) \
         VALUES (x'01', 'access', 1, 'openid', 9, 1)",
        "INSERT INTO oidc_token (token_hash, kind, client_id, person_id, service_party_id, \
         scopes, expires_at, created_at) VALUES (x'02', 'access', 1, 3, 2, 'openid', 9, 1)",
    ] {
        assert!(
            store.execute(both, crate::bind![]).await.is_err(),
            "a token speaks for exactly one of a person and a service"
        );
    }
}
