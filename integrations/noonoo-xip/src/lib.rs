//! # noonoo-xip
//!
//! The opt-in bridge between the Exosphere Identity Protocol (`xip-core`) and
//! Noonoo's capability gating (`noonoo-caps`).
//!
//! `noonoo-caps` owns the policy and the seam but deliberately knows nothing
//! about cryptography or identity. This crate is the other half: it takes a
//! *signed* XIP [`CapabilityToken`], verifies it, and — only if the signature
//! and the token's `nbf`/`exp` window both hold — admits the grants it carries
//! into an [`XipAuthorizer`], which implements the [`Authorizer`] seam.
//!
//! ## Why it is a separate, opt-in crate
//!
//! Verifying a token needs Ed25519, SHA-2, `chrono` and `bs58`. None of that
//! belongs in an engine that must build with **zero third-party dependencies**.
//! So this crate lives *outside* the Noonoo workspace and is compiled only when
//! a host chooses to authenticate agents. The engine's default build never sees
//! it.
//!
//! ## The honest caveat (from XIP's own threat model)
//!
//! On a single box, agents usually run as the same uid with no isolation, so a
//! co-resident process can read the signing key and mint tokens that verify.
//! Verification buys **integrity** and **future federation**; it is **not**
//! authenticity against a co-resident agent. Do not present it as more.
//!
//! ```no_run
//! use chrono::{Duration, Utc};
//! use noonoo_caps::{Action, Authorizer, Scope, Subject};
//! use noonoo_xip::XipAuthorizer;
//! use xip_core::capability::{CapabilityScope, CapabilityTokenBuilder};
//! use xip_core::identity::XipIdentity;
//!
//! let issuer = XipIdentity::generate();
//! let agent = XipIdentity::generate();
//! let token = CapabilityTokenBuilder::new(
//!     agent.did().to_string(),
//!     issuer.did().to_string(),
//!     "noonoo.dom",
//!     CapabilityScope::Path("#secret".to_string()),
//! )
//! .expires_at(Utc::now() + Duration::minutes(5))
//! .build(issuer.signing_key())
//! .unwrap();
//!
//! let mut authorizer = XipAuthorizer::new();
//! authorizer.admit(&token).unwrap();
//!
//! let subject = Subject::new(agent.did());
//! assert!(authorizer
//!     .authorize(&subject, Action::Read, &Scope::path("noonoo.dom", "#secret"))
//!     .is_allowed());
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use chrono::{DateTime, Utc};
use noonoo_caps::{Action, Actions, Authorizer, Decision, Grant, Resource, Scope, Subject};
use serde_json::Value;
use xip_core::capability::{CapabilityScope, CapabilityToken};
use xip_core::{ProtoResult, verifying_key_from_did};

/// An [`Authorizer`] fed by verified XIP capability tokens.
///
/// Each admitted token contributes one [`Grant`] keyed by the token's audience
/// (the subject it was issued *to*). Verification is the only way to add a
/// grant, so every decision this makes rests on a signature that checked out.
#[derive(Debug, Default)]
pub struct XipAuthorizer {
    grants: Vec<Grant>,
}

impl XipAuthorizer {
    /// An authorizer that has verified nothing and therefore denies everything.
    pub fn new() -> Self {
        Self::default()
    }

    /// Verify `token` against the key derived from its issuer's `did:key`, check
    /// its validity window at `now`, and admit its grant.
    ///
    /// Returns the `xip-core` error unchanged if the signature is wrong, the
    /// issuer DID is not a `did:key`, or the token is not yet valid or expired.
    pub fn admit_at(&mut self, token: &CapabilityToken, now: DateTime<Utc>) -> ProtoResult<()> {
        let verifying_key = verifying_key_from_did(token.issuer())?;
        token.verify(&verifying_key)?;
        token.is_valid(Some(now))?;
        self.grants.push(grant_from(token));
        Ok(())
    }

    /// [`XipAuthorizer::admit_at`] at the current time.
    pub fn admit(&mut self, token: &CapabilityToken) -> ProtoResult<()> {
        self.admit_at(token, Utc::now())
    }

    /// The grants admitted so far, one per verified token.
    pub fn grants(&self) -> &[Grant] {
        &self.grants
    }

    /// Whether no token has been admitted.
    pub fn is_empty(&self) -> bool {
        self.grants.is_empty()
    }
}

impl Authorizer for XipAuthorizer {
    fn authorize(&self, subject: &Subject, action: Action, required: &Scope) -> Decision {
        if self
            .grants
            .iter()
            .any(|grant| grant.covers(subject, action, required))
        {
            Decision::Allow
        } else {
            Decision::Deny
        }
    }
}

/// Translate a verified token into a Noonoo grant.
///
/// The subject is the token's audience; the capability and resource come from
/// the token's `capability` and `scope`.
fn grant_from(token: &CapabilityToken) -> Grant {
    Grant::new(
        Subject::new(token.audience().as_str()),
        Scope::new(token.capability(), resource_from(token.scope())),
        actions_from(token),
    )
}

/// Map a XIP `CapabilityScope` onto a Noonoo [`Resource`].
fn resource_from(scope: &CapabilityScope) -> Resource {
    match scope {
        CapabilityScope::Global => Resource::Any,
        CapabilityScope::Service(name) => Resource::named(name.clone()),
        CapabilityScope::Resource(name) => Resource::named(name.clone()),
        CapabilityScope::Path(path) => Resource::path(path.clone()),
        CapabilityScope::Network(network) => Resource::named(network.host.clone()),
        // Custom scopes are opaque to Noonoo; the capability name still bounds
        // the grant, so the resource is left open within it.
        CapabilityScope::Custom(_) => Resource::Any,
    }
}

/// XIP tokens authenticate a capability but carry no verb (read vs write).
///
/// A host may narrow the resulting grant with token metadata key
/// `"noonoo.actions"` — an array of `"read"` and/or `"write"`. When the key is
/// absent the token grants the whole capability (read and write); when it is
/// present but names nothing usable the grant is empty.
fn actions_from(token: &CapabilityToken) -> Actions {
    match token.metadata.get("noonoo.actions") {
        None => Actions::ALL,
        Some(value) => value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .filter_map(parse_action)
                    .collect()
            })
            .unwrap_or(Actions::NONE),
    }
}

fn parse_action(name: &str) -> Option<Action> {
    match name {
        "read" => Some(Action::Read),
        "write" => Some(Action::Write),
        _ => None,
    }
}

/// The version of this crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
