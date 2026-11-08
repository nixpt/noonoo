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

/// The fixture with `#secret` guarded, plus the secret `<p>`.
fn guarded() -> (Document, Gate, NodeId, NodeId, NodeId) {
    let (document, public, secret) = fixture();
    let mut gate = Gate::new();
    gate.guard(secret, secret_scope());
    let secret_p = document.tree().children(secret)[0];
    (document, gate, public, secret, secret_p)
}

#[test]
fn set_text_is_gated_by_the_target_node() {
    let (mut document, gate, public, secret, _) = guarded();
    let subject = Subject::anonymous();
    let policy = Policy::new();
    let mut writer = GatedMutator::new(&mut document, &gate, &policy, &subject);
    assert!(matches!(
        writer.set_text(secret, "overwritten"),
        Err(CapsError::Denied(_))
    ));
    writer.set_text(public, "fine").unwrap();
    assert_eq!(document.text_content(public), "fine");
    assert_eq!(document.text_content(secret), "classified");
}

#[test]
fn replace_children_is_gated_by_the_parent() {
    let (mut document, gate, public, secret, _) = guarded();
    let fresh = document.create_element("span");
    let subject = Subject::anonymous();
    let policy = Policy::new();
    let mut writer = GatedMutator::new(&mut document, &gate, &policy, &subject);
    assert!(matches!(
        writer.replace_children(secret, &[fresh]),
        Err(CapsError::Denied(_))
    ));
    writer.replace_children(public, &[fresh]).unwrap();
    assert_eq!(document.tree().children(public), &[fresh]);
}

#[test]
fn a_node_cannot_be_moved_out_of_a_guarded_subtree() {
    // Moving detaches the node from its old parent, which is a write there:
    // without this, appending a secret node into an open parent would let
    // a subject who cannot write `#secret` take it out.
    let (mut document, gate, public, secret, secret_p) = guarded();
    let public_p = document.tree().children(public)[0];
    let subject = Subject::anonymous();
    let policy = Policy::new();
    let mut writer = GatedMutator::new(&mut document, &gate, &policy, &subject);
    assert!(matches!(
        writer.append_child(public, secret_p),
        Err(CapsError::Denied(_))
    ));
    assert!(matches!(
        writer.insert_before(public_p, secret_p),
        Err(CapsError::Denied(_))
    ));
    assert!(matches!(
        writer.replace_children(public, &[secret_p]),
        Err(CapsError::Denied(_))
    ));
    assert_eq!(
        document.tree().children(secret),
        &[secret_p],
        "still in place"
    );
}

#[test]
fn replacing_children_cannot_drop_a_guarded_child() {
    // `remove` needs write on the node removed; replacing an open parent's
    // children removes each of them, so the same rule applies.
    let (mut document, mut gate, public, _, _) = guarded();
    let public_p = document.tree().children(public)[0];
    gate.guard(public_p, Scope::path(CAP, "#public p"));
    let fresh = document.create_element("span");
    let subject = Subject::anonymous();
    let policy = Policy::new();
    let mut writer = GatedMutator::new(&mut document, &gate, &policy, &subject);
    assert!(matches!(
        writer.replace_children(public, &[fresh]),
        Err(CapsError::Denied(_))
    ));
    assert!(matches!(
        writer.set_text(public, "wiped"),
        Err(CapsError::Denied(_))
    ));
    assert_eq!(document.tree().children(public), &[public_p]);
}

#[test]
fn removing_a_subtree_needs_write_on_every_guard_inside_it() {
    // `<body>` is open, but `#secret` inside it is guarded: removing the
    // open ancestor takes the guarded content with it, so it is refused.
    let document = noonoo_html::parse(
        r#"<main id="open"><div id="secret"><p>classified</p></div><p>x</p></main><aside id="plain"><p>y</p></aside>"#,
    );
    let mut document = document;
    let open = document.get_element_by_id("open").unwrap();
    let secret = document.get_element_by_id("secret").unwrap();
    let plain = document.get_element_by_id("plain").unwrap();
    let mut gate = Gate::new();
    gate.guard(secret, secret_scope());

    let root = document.root();
    let fresh = document.create_element("span");
    {
        let subject = Subject::anonymous();
        let policy = Policy::new();
        let mut writer = GatedMutator::new(&mut document, &gate, &policy, &subject);
        assert!(matches!(writer.remove(open), Err(CapsError::Denied(_))));
        // The same rule for the writes that drop children.
        assert!(matches!(
            writer.replace_children(root, &[fresh]),
            Err(CapsError::Denied(_))
        ));
        assert!(matches!(
            writer.set_text(root, "wiped"),
            Err(CapsError::Denied(_))
        ));
        // A subtree with no guard inside is still removable.
        writer.remove(plain).unwrap();
    }
    assert!(document.is_connected(secret));

    // With write on the inner guard, the open ancestor can go.
    let editor = Subject::new("did:exo:editor");
    let policy = Policy::new().grant(Grant::new(editor.clone(), secret_scope(), Actions::ALL));
    let mut writer = GatedMutator::new(&mut document, &gate, &policy, &editor);
    writer.remove(open).unwrap();
}

#[test]
fn style_property_writes_are_gated() {
    let (mut document, public, secret) = fixture();
    let mut gate = Gate::new();
    gate.guard(secret, secret_scope());
    let subject = Subject::anonymous();
    let policy = Policy::new();
    let mut writer = GatedMutator::new(&mut document, &gate, &policy, &subject);

    assert!(matches!(
        writer.set_style_property(secret, "display", "none", false),
        Err(CapsError::Denied(_))
    ));
    assert!(matches!(
        writer.remove_style_property(secret, "display"),
        Err(CapsError::Denied(_))
    ));
    assert!(matches!(
        writer.set_style_property(public, "color", "red; x: y", false),
        Err(CapsError::Style(_))
    ));
    writer
        .set_style_property(public, "color", "red", false)
        .unwrap();
    writer
        .set_style_property(public, "width", "1px", true)
        .unwrap();
    writer.remove_style_property(public, "color").unwrap();

    assert_eq!(document.element(secret).unwrap().attr("style"), None);
    assert_eq!(
        document.element(public).unwrap().attr("style"),
        Some("width: 1px !important")
    );
}

#[test]
fn insert_after_and_replace_with_are_gated() {
    let (mut document, public, secret) = fixture();
    let mut gate = Gate::new();
    gate.guard(secret, secret_scope());
    let subject = Subject::anonymous();
    let policy = Policy::new();
    let fresh = document.create_element("p");
    let other = document.create_element("p");
    let mut writer = GatedMutator::new(&mut document, &gate, &policy, &subject);

    assert!(matches!(
        writer.insert_after(secret, fresh),
        Err(CapsError::Denied(_))
    ));
    assert!(matches!(
        writer.replace_with(secret, fresh),
        Err(CapsError::Denied(_))
    ));
    // Replacing an open node with a guarded one would move the guarded one.
    assert!(matches!(
        writer.replace_with(public, secret),
        Err(CapsError::Denied(_))
    ));
    writer.insert_after(public, fresh).unwrap();
    writer.replace_with(fresh, other).unwrap();
    assert_eq!(
        document.tree().children(document.root()),
        &[public, other, secret]
    );
}

#[test]
fn replace_with_needs_write_on_the_guards_it_removes() {
    let (mut document, public, secret) = fixture();
    let mut gate = Gate::new();
    let inner = document.tree().children(secret)[0];
    gate.guard(inner, secret_scope());
    let subject = Subject::anonymous();
    let policy = Policy::new();
    let fresh = document.create_element("p");
    let mut writer = GatedMutator::new(&mut document, &gate, &policy, &subject);
    assert!(matches!(
        writer.replace_with(secret, fresh),
        Err(CapsError::Denied(_))
    ));
    writer.replace_with(public, fresh).unwrap();
}

#[test]
fn clone_node_needs_read_on_everything_it_copies() {
    let (mut document, public, secret) = fixture();
    let mut gate = Gate::new();
    let inner = document.tree().children(secret)[0];
    gate.guard(inner, secret_scope());
    let subject = Subject::anonymous();
    let policy = Policy::new();
    {
        let mut writer = GatedMutator::new(&mut document, &gate, &policy, &subject);
        // A shallow copy of the open parent is fine; a deep one would copy
        // the guarded child.
        writer.clone_node(secret, false).unwrap();
        assert!(matches!(
            writer.clone_node(secret, true),
            Err(CapsError::Denied(_))
        ));
        assert!(matches!(
            writer.clone_node(inner, false),
            Err(CapsError::Denied(_))
        ));
        writer.clone_node(public, true).unwrap();
    }

    let reader = Subject::new("did:exo:reader");
    let policy = Policy::new().grant(Grant::read(reader.clone(), secret_scope()));
    let mut writer = GatedMutator::new(&mut document, &gate, &policy, &reader);
    let copy = writer.clone_node(secret, true).unwrap();
    assert_eq!(document.text_content(copy), "classified");
}
