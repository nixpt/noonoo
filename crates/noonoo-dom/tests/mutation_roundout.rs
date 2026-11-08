//! The rest of the mutation surface a script bridge needs: `cloneNode`,
//! `after`/`insertAfter`, `replaceWith`, and elements created with
//! attributes.

use noonoo_dom::{Document, MutError, NodeData, NodeId};

/// `<body>` holding `<a>`, `<b>`, `<c>`.
fn abc() -> (Document, NodeId, [NodeId; 3]) {
    let mut doc = Document::new();
    let root = doc.root();
    let body = doc.create_element("body");
    doc.mutate().append_child(root, body).unwrap();
    let kids = ["a", "b", "c"].map(|tag| {
        let el = doc.create_element(tag);
        doc.mutate().append_child(body, el).unwrap();
        el
    });
    (doc, body, kids)
}

#[test]
fn insert_before_moves_an_earlier_sibling_to_the_right_place() {
    let (mut doc, body, [a, b, c]) = abc();
    doc.mutate().insert_before(c, a).unwrap();
    assert_eq!(doc.tree().children(body), &[b, a, c]);
}

#[test]
fn insert_after_places_the_node_after_its_sibling() {
    let (mut doc, body, [a, b, c]) = abc();
    let d = doc.create_element("d");
    doc.mutate().insert_after(c, d).unwrap();
    assert_eq!(doc.tree().children(body), &[a, b, c, d]);
    doc.mutate().insert_after(a, c).unwrap();
    assert_eq!(doc.tree().children(body), &[a, c, b, d]);
    doc.mutate().insert_after(d, a).unwrap();
    assert_eq!(doc.tree().children(body), &[c, b, d, a]);
    doc.mutate().insert_after(b, b).unwrap();
    assert_eq!(doc.tree().children(body), &[c, b, d, a]);
}

#[test]
fn insert_after_checks_its_arguments() {
    let (mut doc, body, [a, ..]) = abc();
    let loose = doc.create_element("x");
    let root = doc.root();
    assert_eq!(
        doc.mutate().insert_after(loose, a),
        Err(MutError::NoParent(loose))
    );
    assert_eq!(
        doc.mutate().insert_after(a, body),
        Err(MutError::Cycle(body))
    );
    assert_eq!(
        doc.mutate().insert_after(a, root),
        Err(MutError::Cycle(root))
    );
}

#[test]
fn replace_with_swaps_the_node_in_place() {
    let (mut doc, body, [a, b, c]) = abc();
    let d = doc.create_element("d");
    doc.mutate().replace_with(b, d).unwrap();
    assert_eq!(doc.tree().children(body), &[a, d, c]);
    assert_eq!(doc.tree().parent(b), None);
    // A sibling can be the replacement: it moves.
    doc.mutate().replace_with(c, a).unwrap();
    assert_eq!(doc.tree().children(body), &[d, a]);
    // Replacing a node with itself changes nothing.
    doc.mutate().replace_with(d, d).unwrap();
    assert_eq!(doc.tree().children(body), &[d, a]);
}

#[test]
fn replace_with_checks_its_arguments() {
    let (mut doc, body, [a, ..]) = abc();
    let loose = doc.create_element("x");
    assert_eq!(
        doc.mutate().replace_with(loose, a),
        Err(MutError::NoParent(loose))
    );
    assert_eq!(
        doc.mutate().replace_with(a, body),
        Err(MutError::Cycle(body))
    );
}

#[test]
fn a_shallow_clone_copies_the_node_but_not_its_children() {
    let mut doc = noonoo_html::parse(r#"<div id="x" class="c">hi <b>there</b></div>"#);
    let div = doc.get_element_by_id("x").unwrap();
    let copy = doc.clone_node(div, false).unwrap();
    assert_ne!(copy, div);
    assert_eq!(doc.tree().parent(copy), None);
    assert!(doc.tree().children(copy).is_empty());
    assert_eq!(doc.element(copy), doc.element(div));
}

#[test]
fn a_deep_clone_copies_the_subtree_detached() {
    let mut doc = noonoo_html::parse(r#"<div id="x">hi <b class="k">there</b><!--c--></div>"#);
    let div = doc.get_element_by_id("x").unwrap();
    let copy = doc.clone_node(div, true).unwrap();
    assert!(!doc.is_connected(copy));
    assert_eq!(doc.text_content(copy), doc.text_content(div));
    let original: Vec<_> = doc.tree().descendants(div);
    let cloned: Vec<_> = doc.tree().descendants(copy);
    assert_eq!(original.len(), cloned.len());
    for (o, c) in original.iter().zip(&cloned) {
        assert_ne!(o, c);
        assert_eq!(doc.node(*o), doc.node(*c));
    }
    // The copy is independent of the original.
    let b = cloned[1];
    doc.mutate().set_attribute(b, "class", "changed").unwrap();
    assert_eq!(doc.element(original[1]).unwrap().attr("class"), Some("k"));
}

/// Deep enough that a recursive copy would put the stack at risk.
#[test]
fn a_deep_clone_of_a_very_deep_tree_does_not_overflow() {
    let mut doc = Document::new();
    let top = doc.create_element("div");
    let mut cursor = top;
    for _ in 0..20_000 {
        let next = doc.create_element("div");
        doc.mutate().append_child(cursor, next).unwrap();
        cursor = next;
    }
    let copy = doc.clone_node(top, true).unwrap();
    let mut depth = 0;
    let mut cursor = copy;
    while let [child] = doc.tree().children(cursor) {
        cursor = *child;
        depth += 1;
    }
    assert_eq!(depth, 20_000);
}

#[test]
fn cloning_a_missing_node_is_an_error() {
    let (mut doc, _, [a, ..]) = abc();
    let mut other = Document::new();
    for _ in 0..10 {
        other.create_element("p");
    }
    let far = other.create_element("p");
    assert_eq!(doc.clone_node(far, true), Err(MutError::NoSuchNode(far)));
    assert!(doc.clone_node(a, true).is_ok());
}

#[test]
fn create_element_with_attributes() {
    let mut doc = Document::new();
    let el = doc.create_element_with("A", [("HREF", "/x"), ("class", "k"), ("href", "/y")]);
    let data = doc.element(el).unwrap();
    assert_eq!(data.name, "a");
    let attrs: Vec<_> = data
        .attrs
        .iter()
        .map(|a| (a.name.as_str(), a.value.as_str()))
        .collect();
    assert_eq!(attrs, [("href", "/y"), ("class", "k")]);
    assert!(matches!(doc.node(el), Some(NodeData::Element(_))));
}

#[test]
fn upcoming_ids_are_the_ids_the_next_inserts_get() {
    let (mut doc, _, _) = abc();
    let first = doc.tree().upcoming_id(0);
    let second = doc.tree().upcoming_id(1);
    assert_eq!(doc.create_element("x"), first);
    assert_eq!(doc.create_text("y"), second);
}

#[test]
fn a_cloned_document_is_independent() {
    let (doc, body, [a, b, c]) = abc();
    let mut copy = doc.clone();
    assert_eq!(copy.tree().children(body), &[a, b, c]);
    copy.mutate().remove(b).unwrap();
    assert_eq!(copy.tree().children(body), &[a, c]);
    assert_eq!(doc.tree().children(body), &[a, b, c]);
}
