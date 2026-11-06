//! The capability vocabulary: subjects, scopes, actions, and grants.
//!
//! The shape deliberately mirrors the Exosphere Identity Protocol's
//! `(capability, CapabilityScope)` pair — a capability name plus a resource
//! that is global, exact, or path-prefixed — without depending on it, so gating
//! works whether or not the cryptographic backend is present.

use core::fmt;

/// Who is acting: a DID, a uid, a role, or the anonymous user.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Subject(String);

impl Subject {
    /// A subject with the given identity string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The unauthenticated subject. It matches no [`Grant`] unless one is
    /// written for it explicitly.
    pub fn anonymous() -> Self {
        Self::new("anonymous")
    }

    /// The identity string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is [`Subject::anonymous`].
    pub fn is_anonymous(&self) -> bool {
        self.0 == "anonymous"
    }
}

impl From<&str> for Subject {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for Subject {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The resource a [`Scope`] ranges over — the three levels XIP distinguishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resource {
    /// Every resource within the capability (XIP `Global`).
    Any,
    /// Exactly this name (XIP `Service` / `Resource`).
    Named(String),
    /// A path and everything beneath it (XIP `Path`).
    Path(String),
}

impl Resource {
    /// A resource that matches anything.
    pub fn any() -> Self {
        Resource::Any
    }

    /// A resource matched by exact name.
    pub fn named(name: impl Into<String>) -> Self {
        Resource::Named(name.into())
    }

    /// A resource matched by path prefix.
    pub fn path(prefix: impl Into<String>) -> Self {
        Resource::Path(prefix.into())
    }

    /// Whether a grant over `self` satisfies a requirement for `required`.
    pub fn grants(&self, required: &Resource) -> bool {
        match self {
            Resource::Any => true,
            Resource::Named(mine) => matches!(required, Resource::Named(other) if mine == other),
            Resource::Path(prefix) => {
                matches!(required, Resource::Path(path) if path.starts_with(prefix.as_str()))
            }
        }
    }
}

impl fmt::Display for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Resource::Any => f.write_str("*"),
            Resource::Named(name) => f.write_str(name),
            Resource::Path(path) => write!(f, "{path}…"),
        }
    }
}

/// A capability name and the resource it ranges over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    /// The capability, e.g. `"noonoo.dom"`. `"*"` matches every capability.
    pub capability: String,
    /// The resource within that capability.
    pub resource: Resource,
}

impl Scope {
    /// A scope over an explicit resource.
    pub fn new(capability: impl Into<String>, resource: Resource) -> Self {
        Self {
            capability: capability.into(),
            resource,
        }
    }

    /// A scope over every resource in the capability.
    pub fn any(capability: impl Into<String>) -> Self {
        Self::new(capability, Resource::Any)
    }

    /// A scope over one named resource.
    pub fn named(capability: impl Into<String>, name: impl Into<String>) -> Self {
        Self::new(capability, Resource::named(name))
    }

    /// A scope over a path and its descendants.
    pub fn path(capability: impl Into<String>, prefix: impl Into<String>) -> Self {
        Self::new(capability, Resource::path(prefix))
    }

    /// Whether a grant over `self` satisfies a requirement for `required`.
    pub fn grants(&self, required: &Scope) -> bool {
        (self.capability == "*" || self.capability == required.capability)
            && self.resource.grants(&required.resource)
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} over {}", self.capability, self.resource)
    }
}

/// An action within a scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Read a node's data or text.
    Read,
    /// Change a node (attributes, children, structure).
    Write,
}

impl Action {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Read => "read",
            Action::Write => "write",
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A set of actions, kept as a small bitset so the crate needs no dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actions(u8);

impl Actions {
    /// No actions.
    pub const NONE: Actions = Actions(0);
    /// Read only.
    pub const READ: Actions = Actions(0b01);
    /// Write only.
    pub const WRITE: Actions = Actions(0b10);
    /// Read and write.
    pub const ALL: Actions = Actions(0b11);

    /// Whether `action` is in the set.
    pub fn contains(self, action: Action) -> bool {
        self.0 & mask(action) != 0
    }

    /// The union of two sets.
    pub fn union(self, other: Actions) -> Actions {
        Actions(self.0 | other.0)
    }

    /// Whether the set is empty.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

fn mask(action: Action) -> u8 {
    match action {
        Action::Read => 0b01,
        Action::Write => 0b10,
    }
}

impl From<Action> for Actions {
    fn from(action: Action) -> Self {
        Actions(mask(action))
    }
}

impl FromIterator<Action> for Actions {
    fn from_iter<T: IntoIterator<Item = Action>>(iter: T) -> Self {
        iter.into_iter()
            .fold(Actions::NONE, |acc, action| acc.union(action.into()))
    }
}

/// A permission: `subject` may perform `actions` within `scope`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    /// The subject the grant is issued to.
    pub subject: Subject,
    /// The scope the grant applies within.
    pub scope: Scope,
    /// The actions the subject may perform.
    pub actions: Actions,
}

impl Grant {
    /// A grant for an explicit action set.
    pub fn new(subject: impl Into<Subject>, scope: Scope, actions: Actions) -> Self {
        Self {
            subject: subject.into(),
            scope,
            actions,
        }
    }

    /// A read-only grant.
    pub fn read(subject: impl Into<Subject>, scope: Scope) -> Self {
        Self::new(subject, scope, Actions::READ)
    }

    /// A read-and-write grant.
    pub fn all(subject: impl Into<Subject>, scope: Scope) -> Self {
        Self::new(subject, scope, Actions::ALL)
    }

    /// Whether this grant authorises `action` by `subject` over `required`.
    pub fn covers(&self, subject: &Subject, action: Action, required: &Scope) -> bool {
        &self.subject == subject && self.actions.contains(action) && self.scope.grants(required)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_resource_grants_every_level() {
        let any = Resource::Any;
        assert!(any.grants(&Resource::named("a")));
        assert!(any.grants(&Resource::path("/x")));
    }

    #[test]
    fn path_resource_grants_its_descendants_only() {
        let scope = Scope::path("noonoo.dom", "#page/");
        assert!(scope.grants(&Scope::path("noonoo.dom", "#page/body")));
        assert!(scope.grants(&Scope::path("noonoo.dom", "#page/")));
        assert!(!scope.grants(&Scope::path("noonoo.dom", "#other")));
    }

    #[test]
    fn named_resource_is_exact() {
        let scope = Scope::named("notes", "spec");
        assert!(scope.grants(&Scope::named("notes", "spec")));
        assert!(!scope.grants(&Scope::named("notes", "spec.md")));
    }

    #[test]
    fn capability_must_match_outside_the_wildcard() {
        assert!(!Scope::any("a").grants(&Scope::any("b")));
        assert!(Scope::any("*").grants(&Scope::any("b")));
    }

    #[test]
    fn grant_covers_action_subject_and_scope() {
        let grant = Grant::read("alice", Scope::path("noonoo.dom", "#secret"));
        assert!(grant.covers(
            &Subject::new("alice"),
            Action::Read,
            &Scope::path("noonoo.dom", "#secret/ref")
        ));
        assert!(!grant.covers(
            &Subject::new("alice"),
            Action::Write,
            &Scope::path("noonoo.dom", "#secret")
        ));
        assert!(!grant.covers(
            &Subject::new("bob"),
            Action::Read,
            &Scope::path("noonoo.dom", "#secret")
        ));
    }

    #[test]
    fn actions_collect() {
        let set: Actions = [Action::Read, Action::Write].into_iter().collect();
        assert_eq!(set, Actions::ALL);
        assert!(set.contains(Action::Read) && set.contains(Action::Write));
    }
}
