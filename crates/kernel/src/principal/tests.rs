//! Resolving a cookie, expanding it, and keeping the answer warm.

use super::*;
use crate::bind;
use crate::domain::{SESSION_TTL, Token};
use crate::store::Reads;
use crate::testing::Local;

#[tokio::test]
async fn an_unknown_token_is_nobody_rather_than_an_error() {
    let harness = Local::new();
    let resolved = resolve(harness.store(), &Token::mint())
        .await
        .expect("resolving is not a refusal");
    assert_eq!(resolved.principal, Principal::Anonymous);
    assert_eq!(resolved.expires_at, 0);
}

#[tokio::test]
async fn a_live_session_resolves_to_its_identity_and_the_party_it_acts_as() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "resolve@example.test")
        .await
        .expect("registers");
    let resolved = resolve(harness.store(), &who.token)
        .await
        .expect("resolves");

    let Principal::Member {
        identity,
        acting_as,
        session,
        ..
    } = resolved.principal
    else {
        panic!("a live session is a member");
    };
    assert_eq!(identity.get(), 1);
    assert_eq!(acting_as.get(), 1);
    assert_eq!(session.get(), 1);
    assert_eq!(
        resolved.expires_at,
        crate::testing::TEST_EPOCH + SESSION_TTL
    );
    assert!(!resolved.wants_renewal, "a fresh session needs no write");
}

#[tokio::test]
async fn a_session_past_half_its_life_asks_for_a_renewal_without_taking_one() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "renew@example.test")
        .await
        .expect("registers");
    harness.advance(crate::domain::RENEW_AFTER + 1);

    let before: Vec<crate::store::Count> = harness
        .store()
        .query("SELECT expires_at AS n FROM session", bind![])
        .await
        .expect("query runs");
    let resolved = resolve(harness.store(), &who.token)
        .await
        .expect("resolves");
    let after: Vec<crate::store::Count> = harness
        .store()
        .query("SELECT expires_at AS n FROM session", bind![])
        .await
        .expect("query runs");

    assert!(resolved.wants_renewal);
    assert_eq!(
        before[0].0, after[0].0,
        "resolving is a read; the renewal is the observation lane's to write"
    );
}

#[tokio::test]
async fn expanding_anonymous_gives_only_public() {
    let harness = Local::new();
    let set = expand(harness.store(), &Principal::Anonymous)
        .await
        .expect("expands");
    assert_eq!(set.len(), 1, "public, and nothing else");
    assert!(!set.authenticated);
    assert_eq!(set.person, None);
}

#[tokio::test]
async fn expanding_a_bearer_gives_the_link_and_not_authentication() {
    let harness = Local::new();
    let set = expand(
        harness.store(),
        &Principal::Bearer {
            link: crate::ids::Id::new(4),
        },
    )
    .await
    .expect("expands");
    assert_eq!(set.link, Some(crate::ids::Id::new(4)));
    assert!(
        !set.authenticated,
        "holding a link is not being signed in, and a route must be able to tell"
    );
}

#[tokio::test]
async fn expanding_a_member_gives_the_person_its_groups_and_its_organizations() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "expand@example.test")
        .await
        .expect("registers");
    let person = who.principal.acting_as().expect("acts as somebody");

    // K2 writes these rows; the shape they land in is already the one the
    // expansion reads, which is what stops that leg touching this code.
    for (kind, name) in [("organization", "Isoastra"), ("group", "Cohort")] {
        harness
            .store()
            .execute(
                "INSERT INTO party (kind, display_name, status, created_at) \
                 VALUES ($1, $2, 'active', $3)",
                bind![kind, name, crate::testing::TEST_EPOCH],
            )
            .await
            .expect("insert runs");
    }
    harness
        .store()
        .execute(
            "INSERT INTO membership (group_id, party_id, role, at) \
             SELECT id, $1, 'member', $2 FROM party WHERE kind <> 'person'",
            bind![person, crate::testing::TEST_EPOCH],
        )
        .await
        .expect("insert runs");

    let set = expand(harness.store(), &who.principal)
        .await
        .expect("expands");
    assert_eq!(set.person, Some(person));
    assert!(set.authenticated);
    assert_eq!(set.organizations.len(), 1);
    assert_eq!(set.groups.len(), 1);
    assert_eq!(
        set.len(),
        6,
        "public, authenticated, identity, person, one org, one group"
    );
}

#[tokio::test]
async fn a_warm_resolution_answers_the_same_as_a_cold_one() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "warm@example.test")
        .await
        .expect("registers");
    let cache = PrincipalCache::new(16);

    let cold = resolve_cached(harness.store(), &cache, &who.token)
        .await
        .expect("resolves");
    assert_eq!(cache.len(), 1);
    let warm = resolve_cached(harness.store(), &cache, &who.token)
        .await
        .expect("resolves");
    assert_eq!(cold, warm);
}

#[tokio::test]
async fn a_cached_resolution_stops_working_when_the_session_runs_out() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "stale-cache@example.test")
        .await
        .expect("registers");
    let cache = PrincipalCache::new(16);
    resolve_cached(harness.store(), &cache, &who.token)
        .await
        .expect("resolves");

    harness.advance(SESSION_TTL + 1);
    let resolved = resolve_cached(harness.store(), &cache, &who.token)
        .await
        .expect("resolves");
    assert_eq!(
        resolved.principal,
        Principal::Anonymous,
        "expiry needs no event to invalidate on, so the cache checks it"
    );
}

#[tokio::test]
async fn a_command_touching_an_identity_evicts_its_cached_resolution() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "evict@example.test")
        .await
        .expect("registers");
    let cache = PrincipalCache::new(16);
    resolve_cached(harness.store(), &cache, &who.token)
        .await
        .expect("resolves");
    assert_eq!(cache.len(), 1);

    // What the feed consumer does with the event it just read.
    let events = crate::feed::Feed::read(&harness.feed, 0, 16)
        .await
        .expect("reads");
    for committed in &events {
        if let Some(identity) = committed.event.touches_identity() {
            cache.forget_identity(identity);
        }
        if let Some(person) = committed.event.touches_person() {
            cache.forget_person(person);
        }
    }
    assert!(cache.is_empty(), "the registration event evicted it");
}
