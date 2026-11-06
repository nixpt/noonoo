//! End-to-end gating over a real parsed document.

use noonoo_caps::{
    Action, Actions, AllowAll, CapsError, Gate, GatedDocument, GatedMutator, Grant, Policy, Scope,
    Subject,
};
use noonoo_dom::{Document, NodeId};

/// A page with a public section and a secret one.
fn fixture() -> (Document, NodeId, NodeId) {
    let document = noonoo_html::parse(
        r#"<div id="public"><p>hello</p></div><div id="secret"><p>classified</p></div>"#,
    );
    let public = document.get_element_by_id("public").unwrap();
    let secret = document.get_element_by_id("secret").unwrap();
    (document, public, secret)
}

const CAP: &str = "noonoo.dom";

fn secret_scope() -> Scope {
    Scope::path(CAP, "#secret")
}

#[test]
fn unguarded_nodes_are_open_to_anonymous() {
    let (document, public, _) = fixture();
    let gate = Gate::new();
    let subject = Subject::anonymous();
    let authorizer = AllowAll;
    let view = GatedDocument::new(&document, &gate, &authorizer, &subject);

    assert!(view.visible(public));
    assert_eq!(view.text(public).unwrap(), "hello");
    assert!(view.visible_descendants(document.root()).len() >= 2);
}

#[test]
fn a_guarded_subtree_is_hidden_from_anonymous() {
    let (document, public, secret) = fixture();
    let mut gate = Gate::new();
    gate.guard(secret, secret_scope());

    let subject = Subject::anonymous();
    let policy = Policy::new();
    let view = GatedDocument::new(&document, &gate, &policy, &subject);

    assert!(view.visible(public));
    assert!(!view.visible(secret));
    assert!(view.text(secret).is_err());

    // Rendering the whole page elides the secret branch entirely.
    let rendered = view.visible_text(document.root());
    assert!(rendered.contains("hello"));
    assert!(!rendered.contains("classified"));

    let visible = view.visible_descendants(document.root());
    assert!(visible.contains(&public));
    assert!(!visible.contains(&secret));
}

#[test]
fn a_read_grant_reveals_exactly_that_scope() {
    let (document, public, secret) = fixture();
    let mut gate = Gate::new();
    gate.guard(secret, secret_scope());

    let dana = Subject::new("did:exo:dana");
    let policy = Policy::new().grant(Grant::read(dana.clone(), secret_scope()));
    let view = GatedDocument::new(&document, &gate, &policy, &dana);

    assert!(view.can(Action::Read, secret));
    assert_eq!(view.text(secret).unwrap(), "classified");
    // A read grant is not a write grant.
    assert!(!view.can(Action::Write, secret));
    // ...and the unguarded public node stays open.
    assert!(view.visible(public));
}

#[test]
fn the_nearest_guard_wins() {
    let (document, public, secret) = fixture();
    let mut gate = Gate::new();
    gate.guard(document.root(), Scope::path(CAP, "#")); // requires "#" for public
    gate.guard(secret, Scope::named(CAP, "top-secret")); // tightens for secret

    let bob = Subject::new("did:exo:bob");
    let policy = Policy::new().grant(Grant::read(bob.clone(), Scope::named(CAP, "top-secret")));
    let view = GatedDocument::new(&document, &gate, &policy, &bob);

    // The nested "top-secret" scope opens the secret branch...
    assert!(view.visible(secret));
    // ...but the broad "#" requirement over `public` was never granted.
    assert!(!view.visible(public));
}

#[test]
fn writes_are_gated_by_the_target_node() {
    let (mut document, public, secret) = fixture();
    let mut gate = Gate::new();
    gate.guard(secret, secret_scope());

    let new_node = document.create_element("p");
    let label = document.create_text("leaked");
    document.mutate().append_child(new_node, label).unwrap();

    // Anonymous: refused.
    {
        let subject = Subject::anonymous();
        let policy = Policy::new();
        let mut writer = GatedMutator::new(&mut document, &gate, &policy, &subject);
        assert!(matches!(
            writer.append_child(secret, new_node),
            Err(CapsError::Denied(_))
        ));
        // An unguarded parent is fine.
        assert!(writer.append_child(public, new_node).is_ok());
    }

    // A read-only grant is still refused.
    {
        let reader = Subject::new("did:exo:reader");
        let policy = Policy::new().grant(Grant::read(reader.clone(), secret_scope()));
        let mut writer = GatedMutator::new(&mut document, &gate, &policy, &reader);
        assert!(matches!(
            writer.set_attribute(secret, "class", "x"),
            Err(CapsError::Denied(_))
        ));
    }

    // A write grant is allowed.
    {
        let editor = Subject::new("did:exo:editor");
        let policy = Policy::new().grant(Grant::new(editor.clone(), secret_scope(), Actions::ALL));
        let mut writer = GatedMutator::new(&mut document, &gate, &policy, &editor);
        writer.set_attribute(secret, "class", "revealed").unwrap();
    }

    assert_eq!(
        document.element(secret).unwrap().attr("class"),
        Some("revealed")
    );
}

#[test]
fn a_mutation_that_is_allowed_can_still_be_invalid() {
    let (mut document, _public, secret) = fixture();
    let mut gate = Gate::new();
    gate.guard(secret, secret_scope());

    let editor = Subject::new("did:exo:editor");
    let policy = Policy::new().grant(Grant::all(editor.clone(), secret_scope()));

    // Appending a text node is authorised, so the borrow is the DOM's.
    let text = document.create_text("t");
    document.mutate().append_child(secret, text).unwrap();

    let mut writer = GatedMutator::new(&mut document, &gate, &policy, &editor);
    // Setting an attribute on a *text node* is authorised but impossible: the
    // failure is the DOM's, not the policy's.
    assert!(matches!(
        writer.set_attribute(text, "x", "y"),
        Err(CapsError::Mutation(_))
    ));
}
