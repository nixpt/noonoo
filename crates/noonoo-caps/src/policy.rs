//! The policy seam: the [`Authorizer`] trait and two owned implementations.
//!
//! The gate calls exactly one method — [`Authorizer::authorize`] — so a host can
//! swap an in-memory [`Policy`] for a signed-token verifier without the engine
//! knowing. Deny is the default; nothing here is allowed by accident.

use core::fmt;

use crate::scope::{Action, Grant, Scope, Subject};

/// An authorization decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// The action is permitted.
    Allow,
    /// The action is refused.
    Deny,
}

impl Decision {
    /// Whether the decision permits the action.
    pub fn is_allowed(self) -> bool {
        matches!(self, Decision::Allow)
    }

    /// Whether the decision refuses the action.
    pub fn is_denied(self) -> bool {
        matches!(self, Decision::Deny)
    }
}

/// Why an action was refused, carrying enough to log it directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denied {
    /// The subject that was refused.
    pub subject: Subject,
    /// The action it attempted.
    pub action: Action,
    /// The scope the target required.
    pub scope: Scope,
}

impl fmt::Display for Denied {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} denied {} on {} (`{}`)",
            self.subject, self.action, self.scope.resource, self.scope.capability
        )
    }
}

impl std::error::Error for Denied {}

/// The single seam a policy backend must implement.
///
/// Implement it for a role table, a config file, or — as `noonoo-xip` does — a
/// verified capability token. The answer is a plain [`Decision`]; how it was
/// reached is the implementor's business.
pub trait Authorizer {
    /// Decide whether `subject` may perform `action` within `required`.
    fn authorize(&self, subject: &Subject, action: Action, required: &Scope) -> Decision;
}

/// An owned authorizer: a flat list of [`Grant`]s, denying by default.
#[derive(Debug, Clone, Default)]
pub struct Policy {
    grants: Vec<Grant>,
}

impl Policy {
    /// An empty policy — which denies everything.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a grant, chainable.
    pub fn grant(mut self, grant: Grant) -> Self {
        self.grants.push(grant);
        self
    }

    /// Add a grant in place.
    pub fn add(&mut self, grant: Grant) {
        self.grants.push(grant);
    }

    /// The grants held by this policy.
    pub fn grants(&self) -> &[Grant] {
        &self.grants
    }
}

impl Authorizer for Policy {
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

/// An authorizer that allows everything.
///
/// Named loudly on purpose: it is the right answer only for a document you are
/// *choosing* not to gate, never a sensible default.
#[derive(Debug, Clone, Copy, Default)]
pub struct AllowAll;

impl Authorizer for AllowAll {
    fn authorize(&self, _subject: &Subject, _action: Action, _required: &Scope) -> Decision {
        Decision::Allow
    }
}

/// An authorizer that denies everything — a sealed document, whoever asks.
#[derive(Debug, Clone, Copy, Default)]
pub struct DenyAll;

impl Authorizer for DenyAll {
    fn authorize(&self, _subject: &Subject, _action: Action, _required: &Scope) -> Decision {
        Decision::Deny
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn public() -> Scope {
        Scope::path("noonoo.dom", "#public")
    }

    fn secret() -> Scope {
        Scope::path("noonoo.dom", "#secret")
    }

    #[test]
    fn empty_policy_denies() {
        let policy = Policy::new();
        assert!(
            policy
                .authorize(&Subject::new("alice"), Action::Read, &public())
                .is_denied()
        );
    }

    #[test]
    fn a_grant_allows_only_its_actions() {
        let policy = Policy::new().grant(Grant::read("alice", secret()));
        assert!(
            policy
                .authorize(&Subject::new("alice"), Action::Read, &secret())
                .is_allowed()
        );
        assert!(
            policy
                .authorize(&Subject::new("alice"), Action::Write, &secret())
                .is_denied()
        );
    }

    #[test]
    fn a_grant_does_not_leak_to_other_subjects_or_scopes() {
        let policy = Policy::new().grant(Grant::read("alice", secret()));
        assert!(
            policy
                .authorize(&Subject::new("bob"), Action::Read, &secret())
                .is_denied()
        );
        assert!(
            policy
                .authorize(&Subject::new("alice"), Action::Read, &public())
                .is_denied()
        );
    }

    #[test]
    fn allow_all_and_deny_all() {
        let subject = Subject::anonymous();
        assert!(
            AllowAll
                .authorize(&subject, Action::Write, &secret())
                .is_allowed()
        );
        assert!(
            DenyAll
                .authorize(&subject, Action::Read, &public())
                .is_denied()
        );
    }
}
