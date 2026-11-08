//! The change log: what the checked mutator records for incremental work.

use noonoo_dom::{Change, Document, NodeId};

/// `<root><p a><b/></p><div/></root>` with the change log emptied.
fn doc() -> (Document, NodeId, NodeId, NodeId) {
    let mut doc = Document::new();
    let root = doc.root();
    let p = doc.create_element("p");
    let b = doc.create_element("b");
    let div = doc.create_element("div");
    let mut m = doc.mutate();
    m.append_child(root, p).unwrap();
    m.append_child(p, b).unwrap();
    m.append_child(root, div).unwrap();
    doc.take_changes();
    (doc, p, b, div)
}

#[test]
fn attribute_edits_record_the_element() {
    let (mut doc, p, _, _) = doc();
    doc.mutate().set_attribute(p, "class", "x").unwrap();
    doc.mutate().remove_attribute(p, "class").unwrap();
    let changes = doc.take_changes();
    assert!(!changes.overflowed());
    assert_eq!(
        changes.iter().collect::<Vec<_>>(),
        vec![&Change::Attributes(p), &Change::Attributes(p)]
    );
    assert!(doc.take_changes().is_empty(), "taking empties the log");
}

#[test]
fn a_move_records_the_old_and_the_new_parent() {
    let (mut doc, p, b, div) = doc();
    doc.mutate().append_child(div, b).unwrap();
    assert_eq!(
        doc.take_changes().iter().collect::<Vec<_>>(),
        vec![&Change::Children(p), &Change::Children(div)]
    );
    doc.mutate().insert_before(p, b).unwrap();
    doc.mutate().insert_after(p, b).unwrap();
    let root = doc.root();
    assert_eq!(
        doc.take_changes().iter().collect::<Vec<_>>(),
        vec![
            &Change::Children(div),
            &Change::Children(root),
            &Change::Children(root),
            &Change::Children(root),
        ]
    );
}

#[test]
fn removing_replacing_and_text_record_their_parents() {
    let (mut doc, p, b, div) = doc();
    let root = doc.root();
    doc.mutate().remove(b).unwrap();
    doc.mutate().replace_with(div, b).unwrap();
    doc.mutate().replace_children(p, &[div]).unwrap();
    assert_eq!(
        doc.take_changes().iter().collect::<Vec<_>>(),
        vec![
            &Change::Children(p),
            &Change::Children(root),
            &Change::Children(p),
        ]
    );
    let text = doc.create_text("a");
    doc.mutate().append_child(div, text).unwrap();
    doc.take_changes();
    doc.mutate().set_text(text, "b").unwrap();
    doc.mutate().set_text(div, "c").unwrap();
    assert_eq!(
        doc.take_changes().iter().collect::<Vec<_>>(),
        vec![&Change::Text(text), &Change::Children(div)]
    );
}

#[test]
fn a_rejected_mutation_records_nothing() {
    let (mut doc, p, b, _) = doc();
    assert!(doc.mutate().append_child(b, p).is_err());
    assert!(doc.take_changes().is_empty());
}

#[test]
fn the_raw_tree_or_a_long_log_means_anything_may_have_changed() {
    let (mut doc, p, _, _) = doc();
    doc.tree_mut();
    let changes = doc.take_changes();
    assert!(changes.overflowed() && !changes.is_empty());
    for _ in 0..=Document::MAX_CHANGES {
        doc.mutate().set_attribute(p, "x", "y").unwrap();
    }
    let changes = doc.take_changes();
    assert!(changes.overflowed());
    assert_eq!(changes.iter().count(), 0, "an overflowed log keeps no list");
}
