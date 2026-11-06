//! Verifying real XIP tokens and gating a real document.

use chrono::{Duration, Utc};
use noonoo_caps::{Action, Authorizer, Gate, GatedDocument, Scope, Subject};
use noonoo_dom::Document;
use noonoo_xip::XipAuthorizer;
use xip_core::CapabilityToken;
use xip_core::capability::{CapabilityScope, CapabilityTokenBuilder, TokenDelegation};
use xip_core::identity::XipIdentity;

const CAP: &str = "noonoo.dom";

/// A token issued by `issuer` to `aud` for `scope`, valid for five minutes.
fn token(issuer: &XipIdentity, aud: &str, scope: CapabilityScope) -> CapabilityToken {
    CapabilityTokenBuilder::new(aud.to_string(), issuer.did().to_string(), CAP, scope)
        .expires_at(Utc::now() + Duration::minutes(5))
        .build(issuer.signing_key())
        .unwrap()
}

/// A token valid for `minutes` — used to script delegation expiry ordering.
fn expiring(
    issuer: &XipIdentity,
    aud: &str,
    scope: CapabilityScope,
    minutes: i64,
) -> CapabilityToken {
    CapabilityTokenBuilder::new(aud.to_string(), issuer.did().to_string(), CAP, scope)
        .expires_at(Utc::now() + Duration::minutes(minutes))
        .build(issuer.signing_key())
        .unwrap()
}

#[test]
fn a_valid_token_grants_exactly_its_scope() {
    let issuer = XipIdentity::generate();
    let agent = XipIdentity::generate();

    let mut authorizer = XipAuthorizer::new();
    assert!(authorizer.is_empty());
    authorizer
        .admit(&token(
            &issuer,
            agent.did(),
            CapabilityScope::Path("#secret".to_string()),
        ))
        .unwrap();

    let subject = Subject::new(agent.did());
    // Within the path (and beneath it): allowed.
    assert!(
        authorizer
            .authorize(&subject, Action::Read, &Scope::path(CAP, "#secret"))
            .is_allowed()
    );
    assert!(
        authorizer
            .authorize(&subject, Action::Read, &Scope::path(CAP, "#secret/ref"))
            .is_allowed()
    );
    // Outside the path: denied.
    assert!(
        authorizer
            .authorize(&subject, Action::Read, &Scope::path(CAP, "#public"))
            .is_denied()
    );
    // Different capability: denied.
    assert!(
        authorizer
            .authorize(&subject, Action::Read, &Scope::path("other.cap", "#secret"))
            .is_denied()
    );
    // A different subject: denied.
    assert!(
        authorizer
            .authorize(
                &Subject::anonymous(),
                Action::Read,
                &Scope::path(CAP, "#secret")
            )
            .is_denied()
    );
}

#[test]
fn the_default_grant_is_read_and_write() {
    let issuer = XipIdentity::generate();
    let agent = XipIdentity::generate();

    let mut authorizer = XipAuthorizer::new();
    authorizer
        .admit(&token(
            &issuer,
            agent.did(),
            CapabilityScope::Path("#secret".to_string()),
        ))
        .unwrap();

    let subject = Subject::new(agent.did());
    assert!(
        authorizer
            .authorize(&subject, Action::Write, &Scope::path(CAP, "#secret"))
            .is_allowed()
    );
}

#[test]
fn metadata_can_narrow_a_token_to_read_only() {
    let issuer = XipIdentity::generate();
    let agent = XipIdentity::generate();

    let token = CapabilityTokenBuilder::new(
        agent.did().to_string(),
        issuer.did().to_string(),
        CAP,
        CapabilityScope::Path("#secret".to_string()),
    )
    .expires_at(Utc::now() + Duration::minutes(5))
    .with_metadata("noonoo.actions", serde_json::json!(["read"]))
    .build(issuer.signing_key())
    .unwrap();

    let mut authorizer = XipAuthorizer::new();
    authorizer.admit(&token).unwrap();

    let subject = Subject::new(agent.did());
    assert!(
        authorizer
            .authorize(&subject, Action::Read, &Scope::path(CAP, "#secret"))
            .is_allowed()
    );
    assert!(
        authorizer
            .authorize(&subject, Action::Write, &Scope::path(CAP, "#secret"))
            .is_denied()
    );
}

#[test]
fn a_token_signed_by_the_wrong_key_is_rejected() {
    let issuer = XipIdentity::generate();
    let forger = XipIdentity::generate();
    let agent = XipIdentity::generate();

    // Claims to be issued by `issuer`, but signed with the forger's key.
    let forged = CapabilityTokenBuilder::new(
        agent.did().to_string(),
        issuer.did().to_string(),
        CAP,
        CapabilityScope::Global,
    )
    .build(forger.signing_key())
    .unwrap();

    let mut authorizer = XipAuthorizer::new();
    assert!(authorizer.admit(&forged).is_err());
    assert!(
        authorizer.is_empty(),
        "a rejected token must not be admitted"
    );
}

#[test]
fn an_expired_token_is_rejected() {
    let issuer = XipIdentity::generate();
    let agent = XipIdentity::generate();

    let expired = CapabilityTokenBuilder::new(
        agent.did().to_string(),
        issuer.did().to_string(),
        CAP,
        CapabilityScope::Global,
    )
    .expires_at(Utc::now() - Duration::minutes(1))
    .build(issuer.signing_key())
    .unwrap();

    let mut authorizer = XipAuthorizer::new();
    assert!(authorizer.admit(&expired).is_err());
}

#[test]
fn a_verified_token_gates_a_document_end_to_end() {
    let issuer = XipIdentity::generate();
    let agent = XipIdentity::generate();

    // A page with a secret branch.
    let mut document = Document::new();
    let secret = document.create_element("secret");
    let root = document.root();
    document.mutate().append_child(root, secret).unwrap();

    let mut gate = Gate::new();
    gate.guard(secret, Scope::path(CAP, "#secret"));

    // Anonymous: the branch is invisible.
    let anonymous = Subject::anonymous();
    let sealed = XipAuthorizer::new();
    let view = GatedDocument::new(&document, &gate, &sealed, &anonymous);
    assert!(!view.visible(secret));

    // The agent presents a verified token for the branch: it opens.
    let mut authorizer = XipAuthorizer::new();
    // The guard requires Path("#secret"), and the token grants it.
    let token = CapabilityTokenBuilder::new(
        agent.did().to_string(),
        issuer.did().to_string(),
        CAP,
        CapabilityScope::Path("#secret".to_string()),
    )
    .expires_at(Utc::now() + Duration::minutes(5))
    .build(issuer.signing_key())
    .unwrap();
    authorizer.admit(&token).unwrap();

    let agent_subject = Subject::new(agent.did());
    let view = GatedDocument::new(&document, &gate, &authorizer, &agent_subject);
    assert!(view.visible(secret));

    // The same token does nothing for a different subject.
    let other = Subject::new(XipIdentity::generate().did());
    let view = GatedDocument::new(&document, &gate, &authorizer, &other);
    assert!(!view.visible(secret));
}

#[test]
fn a_valid_delegation_admits_the_delegate() {
    let issuer = XipIdentity::generate();
    let alice = XipIdentity::generate();
    let bob = XipIdentity::generate();

    // The issuer grants alice Path("#secret"); alice re-issues a *narrower*
    // slice to bob.
    let original = expiring(
        &issuer,
        alice.did(),
        CapabilityScope::Path("#secret".to_string()),
        10,
    );
    let delegated = expiring(
        &alice,
        bob.did(),
        CapabilityScope::Path("#secret/ref".to_string()),
        5,
    );
    let link = TokenDelegation::new(original, delegated, 1);

    let mut authorizer = XipAuthorizer::new();
    authorizer.admit_delegation(&link).unwrap();

    let bob_subject = Subject::new(bob.did());
    assert!(
        authorizer
            .authorize(&bob_subject, Action::Read, &Scope::path(CAP, "#secret/ref"))
            .is_allowed()
    );
    // Outside the narrowed scope: denied.
    assert!(
        authorizer
            .authorize(&bob_subject, Action::Read, &Scope::path(CAP, "#secret"))
            .is_denied()
    );
    // The delegator is not a grant holder by virtue of delegating.
    let alice_subject = Subject::new(alice.did());
    assert!(
        authorizer
            .authorize(
                &alice_subject,
                Action::Read,
                &Scope::path(CAP, "#secret/ref")
            )
            .is_denied()
    );
}

#[test]
fn a_delegation_that_widens_scope_is_rejected() {
    let issuer = XipIdentity::generate();
    let alice = XipIdentity::generate();
    let bob = XipIdentity::generate();

    let original = expiring(
        &issuer,
        alice.did(),
        CapabilityScope::Path("#secret".to_string()),
        10,
    );
    // Broader than the original: the chain check must refuse it.
    let delegated = expiring(&alice, bob.did(), CapabilityScope::Path("#".to_string()), 5);
    let link = TokenDelegation::new(original, delegated, 1);

    let mut authorizer = XipAuthorizer::new();
    assert!(authorizer.admit_delegation(&link).is_err());
    assert!(authorizer.is_empty());
}

#[test]
fn a_delegation_signed_by_a_forger_is_rejected() {
    let issuer = XipIdentity::generate();
    let alice = XipIdentity::generate();
    let forger = XipIdentity::generate();
    let bob = XipIdentity::generate();

    let original = expiring(
        &issuer,
        alice.did(),
        CapabilityScope::Path("#secret".to_string()),
        10,
    );
    // Claims to be issued by alice, but signed by the forger.
    let delegated = CapabilityTokenBuilder::new(
        bob.did().to_string(),
        alice.did().to_string(),
        CAP,
        CapabilityScope::Path("#secret".to_string()),
    )
    .expires_at(Utc::now() + Duration::minutes(5))
    .build(forger.signing_key())
    .unwrap();
    let link = TokenDelegation::new(original, delegated, 1);

    let mut authorizer = XipAuthorizer::new();
    assert!(authorizer.admit_delegation(&link).is_err());
}

#[test]
fn a_delegation_that_outlives_the_original_is_rejected() {
    let issuer = XipIdentity::generate();
    let alice = XipIdentity::generate();
    let bob = XipIdentity::generate();

    let original = expiring(
        &issuer,
        alice.did(),
        CapabilityScope::Path("#secret".to_string()),
        3,
    );
    // Delegated must not outlive the original.
    let delegated = expiring(
        &alice,
        bob.did(),
        CapabilityScope::Path("#secret".to_string()),
        10,
    );
    let link = TokenDelegation::new(original, delegated, 1);

    let mut authorizer = XipAuthorizer::new();
    assert!(authorizer.admit_delegation(&link).is_err());
}

#[test]
fn a_two_hop_chain_admits_only_the_final_delegate() {
    let issuer = XipIdentity::generate();
    let alice = XipIdentity::generate();
    let bob = XipIdentity::generate();
    let carol = XipIdentity::generate();

    let original = expiring(
        &issuer,
        alice.did(),
        CapabilityScope::Path("#secret".to_string()),
        10,
    );
    let middle = expiring(
        &alice,
        bob.did(),
        CapabilityScope::Path("#secret/sub".to_string()),
        8,
    );
    let final_token = expiring(
        &bob,
        carol.did(),
        CapabilityScope::Path("#secret/sub".to_string()),
        6,
    );

    let link1 = TokenDelegation::new(original, middle, 1);
    let link2 = TokenDelegation::new(link1.delegated_token.clone(), final_token, 2);

    let mut authorizer = XipAuthorizer::new();
    authorizer.admit_chain(&[link1, link2]).unwrap();

    let carol_subject = Subject::new(carol.did());
    assert!(
        authorizer
            .authorize(
                &carol_subject,
                Action::Read,
                &Scope::path(CAP, "#secret/sub")
            )
            .is_allowed()
    );
    assert!(
        authorizer
            .authorize(&carol_subject, Action::Read, &Scope::path(CAP, "#secret"))
            .is_denied()
    );

    // The intermediate holder is not authorized by the final grant.
    let bob_subject = Subject::new(bob.did());
    assert!(
        authorizer
            .authorize(&bob_subject, Action::Read, &Scope::path(CAP, "#secret/sub"))
            .is_denied()
    );
}

#[test]
fn a_non_contiguous_chain_is_rejected() {
    let issuer = XipIdentity::generate();
    let alice = XipIdentity::generate();
    let bob = XipIdentity::generate();
    let carol = XipIdentity::generate();

    let original = expiring(
        &issuer,
        alice.did(),
        CapabilityScope::Path("#secret".to_string()),
        10,
    );
    let middle = expiring(
        &alice,
        bob.did(),
        CapabilityScope::Path("#secret".to_string()),
        8,
    );
    let link1 = TokenDelegation::new(original, middle, 1);

    // A second link whose original is an unrelated token, not link1's delegate.
    let unrelated = expiring(
        &carol,
        bob.did(),
        CapabilityScope::Path("#secret".to_string()),
        8,
    );
    let dangling = expiring(
        &alice,
        carol.did(),
        CapabilityScope::Path("#secret".to_string()),
        4,
    );
    let link2 = TokenDelegation::new(unrelated, dangling, 2);

    let mut authorizer = XipAuthorizer::new();
    assert!(authorizer.admit_chain(&[link1, link2]).is_err());
    assert!(authorizer.is_empty());
}
