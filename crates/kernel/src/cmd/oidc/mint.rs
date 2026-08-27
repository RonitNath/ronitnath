//! Minting a grant: what a token pair is, how it is signed, and how the client
//! that asked for it proves it is itself.
//!
//! Kept apart from the five commands in [`super::token`] because it is a
//! different question. That file says *when* a grant happens — a code that is
//! live and this client's, a refresh token that has not been used before, a
//! credential that checks out. This one says what a grant *is*: two opaque
//! secrets, an id token signed under the active key, a `sub` that may have to
//! be minted, and the rows that record all of it.
//!
//! Everything here runs before the batch, and that is the ordering the id
//! token forces: its `sub` may be a subject this person has never had at this
//! sector, and minting one is a row that goes in the same batch as the tokens
//! — so the string has to exist before the batch is built.

use rn_api::oidc::Scope;

use super::{client_id_of, client_of};
use crate::bind;
use crate::cmd::{Batch, Ctx};
use crate::domain::Token;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{Id, Session};
use crate::oidc::{ClientRow, Credential, client as registry, jwt, key, subject, token as tokens};
use crate::store::{Reads, Sql, Value};

/// What the token endpoint hands back.
#[derive(Debug, Default)]
pub struct Issued {
    /// The access token.
    pub access: Option<Token>,
    /// The refresh token, when `offline_access` was consented to.
    pub refresh: Option<Token>,
    /// The signed id token, for the two grants that mint one.
    pub id_token: Option<String>,
    /// Seconds the access token has left.
    pub expires_in: i64,
    /// The `scope` string that was actually granted.
    pub scopes: String,
}

/// [`Issued`], and the event that produced it.
#[derive(Debug)]
pub struct Granted {
    /// The event and its offset.
    pub committed: Committed,
    /// The secrets, on the call that minted them.
    pub issued: Issued,
}

/// When the session behind a code last proved a factor.
///
/// `session.created_at`, because a session is minted by proving one and
/// nothing since has re-proved it. `max_age` and `auth_time` are both read
/// against this.
pub(super) async fn auth_time_of(reads: &impl Reads, session: Id<Session>) -> Outcome<i64> {
    struct At(i64);
    impl crate::store::FromRow for At {
        fn from_row(row: &mut impl crate::store::Cursor) -> Result<Self, crate::store::RowError> {
            Ok(Self(row.int("created_at")?))
        }
    }
    match reads
        .query_opt::<At>(
            "SELECT created_at FROM session WHERE id = $1",
            bind![session],
        )
        .await?
    {
        Some(at) => Ok(at.0),
        None => decline(),
    }
}

/// The client, having proved it is itself by the method its registration
/// names.
pub(super) async fn authenticated_client<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    client_id: &str,
    secret: Option<&str>,
    assertion: Option<&str>,
) -> Outcome<ClientRow> {
    let client = client_of(ctx, client_id).await?;
    registry::authenticate(
        &ctx.store.reads(),
        &client,
        &Credential { secret, assertion },
        &ctx.provider.token_endpoint(),
        ctx.now(),
    )
    .await?;
    Ok(client)
}

/// Who a token is being minted for.
pub(super) enum Grantee {
    /// A person, through a session that will take it with it.
    Person {
        person: crate::ids::Id<crate::ids::Person>,
        session: Id<Session>,
        auth_time: i64,
        nonce: Option<String>,
    },
    /// A client's own service party.
    Service {
        party: crate::ids::Id<crate::ids::Person>,
    },
}

/// Everything a grant produces, before any of it is written.
pub(super) struct Minting {
    access: Token,
    refresh: Option<Token>,
    id_token: Option<String>,
    scopes: String,
    expires_at: i64,
    subject_stmt: Option<crate::store::Stmt>,
    grantee: MintedFor,
    /// The rotation family a refresh token joins. `None` starts one, and the
    /// row's own id becomes it.
    pub(super) family: Option<i64>,
}

struct MintedFor {
    session: Option<Id<Session>>,
    person: Option<crate::ids::Id<crate::ids::Person>>,
    service: Option<crate::ids::Id<crate::ids::Person>>,
    auth_time: Option<i64>,
    nonce: Option<String>,
    client: crate::ids::Id<crate::ids::OidcClient>,
}

/// Mint the tokens a grant produces, and sign the id token, without writing
/// anything.
///
/// It is done before the batch because the id token's `sub` may be a subject
/// this person has never had at this sector — and minting one is a row that
/// goes in the same batch as the tokens, so the string has to exist first.
pub(super) async fn mint_pair<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    client: &ClientRow,
    grantee: Grantee,
    scopes: &[Scope],
    now: i64,
) -> Outcome<Minting> {
    let reads = ctx.store.reads();
    let client_id = client_id_of(ctx, client.id);
    let access = Token::mint();
    let refresh = scopes.contains(&Scope::OfflineAccess).then(Token::mint);

    let (id_token, subject_stmt, minted_for) = match grantee {
        Grantee::Person {
            person,
            session,
            auth_time,
            nonce,
        } => {
            let (sub, stmt) = subject::resolve(&reads, client.sector(), person, now).await?;
            let signing = key::active(&reads, ctx.provider.seal()).await?;
            let sid = session.public(reads.ids()).to_string();
            let mut claims = serde_json::json!({
                "iss": ctx.provider.issuer(),
                "sub": sub,
                "aud": client_id,
                "azp": client_id,
                "exp": now + tokens::ACCESS_TTL,
                "iat": now,
                "auth_time": auth_time,
                "sid": sid,
            });
            if let Some(nonce) = &nonce
                && let Some(object) = claims.as_object_mut()
            {
                object.insert("nonce".to_owned(), nonce.clone().into());
            }
            let signed = jwt::sign(&signing.private, &signing.kid, jwt::TYP_JWT, &claims)?;
            (
                Some(signed),
                stmt,
                MintedFor {
                    session: Some(session),
                    person: Some(person),
                    service: None,
                    auth_time: Some(auth_time),
                    nonce,
                    client: client.id,
                },
            )
        }
        // No id token: `client_credentials` has no end user to make claims
        // about, and OpenID Connect Core §3 does not define one for it.
        Grantee::Service { party } => (
            None,
            None,
            MintedFor {
                session: None,
                person: None,
                service: Some(party),
                auth_time: None,
                nonce: None,
                client: client.id,
            },
        ),
    };

    Ok(Minting {
        access,
        refresh,
        id_token,
        scopes: Scope::join(scopes),
        expires_at: now + tokens::ACCESS_TTL,
        subject_stmt,
        grantee: minted_for,
        family: None,
    })
}

/// The statements a minting writes.
pub(super) fn write_pair(batch: &mut Batch, minting: &Minting) {
    if let Some(stmt) = &minting.subject_stmt {
        batch.push(stmt.clone());
    }
    let row = |kind: &'static str, digest: Vec<u8>, expires_at: i64, family: Option<i64>| {
        vec![
            Value::from(digest),
            Value::from(kind),
            Value::from(minting.grantee.client),
            Value::from(minting.grantee.session),
            Value::from(minting.grantee.person),
            Value::from(minting.grantee.service),
            Value::from(family),
            Value::from(minting.scopes.as_str()),
            Value::from(minting.grantee.nonce.clone()),
            Value::from(minting.grantee.auth_time),
            Value::from(expires_at),
            Value::from(minting.expires_at - tokens::ACCESS_TTL),
        ]
    };
    batch.one(
        tokens::INSERT_SQL,
        row(
            "access",
            minting.access.digest().to_vec(),
            minting.expires_at,
            None,
        ),
    );
    if let Some(refresh) = &minting.refresh {
        let inserted = batch.one(
            tokens::INSERT_SQL,
            row(
                "refresh",
                refresh.digest().to_vec(),
                minting.expires_at - tokens::ACCESS_TTL + tokens::REFRESH_TTL,
                minting.family,
            ),
        );
        // The first refresh token of a chain has no family to join, so its
        // family is itself. A second statement, because the rowid does not
        // exist until the insert above has run.
        if minting.family.is_none() {
            batch.any(tokens::SEED_FAMILY_SQL, vec![inserted.column("id")]);
        }
    }
}

/// The reply's secrets — on a fresh mint, and nothing on a replay, for the
/// reason a session token is not reproduced either.
pub(super) fn harvest(fresh: bool, minting: Minting) -> Issued {
    if !fresh {
        return Issued {
            scopes: minting.scopes,
            expires_in: tokens::ACCESS_TTL,
            ..Issued::default()
        };
    }
    Issued {
        access: Some(minting.access),
        refresh: minting.refresh,
        id_token: minting.id_token,
        expires_in: tokens::ACCESS_TTL,
        scopes: minting.scopes,
    }
}
