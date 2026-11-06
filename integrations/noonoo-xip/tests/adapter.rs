//! Verifying real XIP tokens and gating a real document.

use chrono::{Duration, Utc};
use noonoo_caps::{Action, Authorizer, Gate, GatedDocument, Scope, Subject};
use noonoo_dom::Document;
use noonoo_xip::XipAuthorizer;
use xip_core::CapabilityToken;
use xip_core::capability::{CapabilityScope, CapabilityTokenBuilder};
use xip_core::identity::XipIdentity;

const CAP: &str = "noonoo.dom";

/// A token issued by `issuer` to `aud` for `scope`, valid for five minutes.
fn token(issuer: &XipIdentity, aud: &str, scope: CapabilityScope) -> CapabilityToken {
    CapabilityTokenBuilder::new(aud.to_string(), issuer.did().to_string(), CAP, scope)
        .expires_at(Utc::now() + Duration::minutes(5))
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
