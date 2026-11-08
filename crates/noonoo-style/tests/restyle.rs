//! Incremental restyle: after an edit, `Cascade::restyle` brings styles up
//! to date by restyling only what the edit can reach, and the result is the
//! same as computing from scratch.

use noonoo_css::{Color, Stylesheet};
use noonoo_dom::{Change, Document, NodeId};
use noonoo_style::{Cascade, Interaction, Styles, Viewport};

const CSS: &str = r#"
    :root { --accent: navy }
    li { color: black }
    .on { color: red }
    .on + li { color: green }
    .on ~ li { font-size: 20px }
    .on li { color: purple }
    li:last-child { margin-left: 3px }
    li:nth-child(2) { margin-right: 4px }
    .tinted { color: var(--accent) }
    .note::before { content: "note: " attr(data-n) }
    .x { --accent: teal }
    #far { width: 2rem }
"#;

const HTML: &str = r#"<html><body>
    <ul id="list"><li id="a">a</li><li id="b">b<ul><li id="b1">b1</li></ul></li><li id="c">c</li></ul>
    <div id="side" class="x"><p id="t" class="tinted">t</p><p id="n" class="note" data-n="1">n</p></div>
    <p id="far">far</p>
</body></html>"#;

fn page() -> (Document, Cascade) {
    let mut doc = noonoo_html::parse(HTML);
    doc.take_changes();
    let sheet = Stylesheet::parse(CSS).unwrap();
    (doc, Cascade::new(&[sheet], Viewport::DEFAULT))
}

fn id(doc: &Document, name: &str) -> NodeId {
    doc.get_element_by_id(name).unwrap()
}

/// `styles` agree with a fresh compute on every element and generated box.
fn assert_fresh(doc: &Document, cascade: &Cascade, styles: &Styles, interaction: Interaction) {
    let fresh = cascade.compute(doc, interaction);
    for element in doc.elements() {
        assert_eq!(
            styles.get(element),
            fresh.get(element),
            "element {element:?} <{}>",
            doc.element(element).unwrap().name
        );
    }
    assert_eq!(styles.generated(), fresh.generated());
    for generated in fresh.generated() {
        assert_eq!(styles.get(generated.element), fresh.get(generated.element));
    }
}

/// Take the document's changes and restyle; how many elements were styled.
fn restyle(doc: &mut Document, cascade: &Cascade, styles: &mut Styles) -> usize {
    let changes = doc.take_changes();
    cascade.restyle(styles, doc, Interaction::default(), &changes)
}

#[test]
fn a_class_change_restyles_its_siblings_and_descendants() {
    let (mut doc, cascade) = page();
    let mut styles = cascade.compute(&doc, Interaction::default());
    let a = id(&doc, "a");
    doc.mutate().set_attribute(a, "class", "on").unwrap();
    let styled = restyle(&mut doc, &cascade, &mut styles);
    assert_eq!(styled, 5, "the list's descendants: a, b, its ul, b1, c");
    assert_eq!(styles.get(a).unwrap().color, Color::rgb(255, 0, 0));
    assert_eq!(
        styles.get(id(&doc, "b")).unwrap().color,
        Color::rgb(0, 128, 0)
    );
    assert_eq!(styles.get(id(&doc, "c")).unwrap().font_size, 20.0);
    assert_fresh(&doc, &cascade, &styles, Interaction::default());

    let b = id(&doc, "b");
    doc.mutate().set_attribute(b, "class", "on").unwrap();
    restyle(&mut doc, &cascade, &mut styles);
    assert_eq!(
        styles.get(id(&doc, "b1")).unwrap().color,
        Color::rgb(128, 0, 128)
    );
    assert_fresh(&doc, &cascade, &styles, Interaction::default());
}

#[test]
fn inserted_and_removed_children_are_styled_and_move_structural_matches() {
    let (mut doc, cascade) = page();
    let mut styles = cascade.compute(&doc, Interaction::default());
    let list = id(&doc, "list");
    let d = doc.create_element("li");
    let text = doc.create_text("d");
    doc.mutate().append_child(d, text).unwrap();
    doc.mutate().append_child(list, d).unwrap();
    assert_eq!(
        restyle(&mut doc, &cascade, &mut styles),
        6,
        "the list's descendants, not its parent's"
    );
    assert!(styles.get(d).is_some(), "the new element has a style");
    assert_fresh(&doc, &cascade, &styles, Interaction::default());

    let a = id(&doc, "a");
    doc.mutate().remove(a).unwrap();
    restyle(&mut doc, &cascade, &mut styles);
    assert_fresh(&doc, &cascade, &styles, Interaction::default());
}

#[test]
fn custom_properties_reach_the_restyled_subtree() {
    let (mut doc, cascade) = page();
    let mut styles = cascade.compute(&doc, Interaction::default());
    let side = id(&doc, "side");
    doc.mutate()
        .set_attribute(side, "style", "--accent: maroon")
        .unwrap();
    restyle(&mut doc, &cascade, &mut styles);
    assert_eq!(
        styles.get(id(&doc, "t")).unwrap().color,
        Color::rgb(128, 0, 0)
    );
    assert_fresh(&doc, &cascade, &styles, Interaction::default());
}

#[test]
fn generated_content_follows_its_owner_and_keeps_its_ids_lined_up() {
    let (mut doc, cascade) = page();
    let mut styles = cascade.compute(&doc, Interaction::default());
    let n = id(&doc, "n");
    doc.mutate().set_attribute(n, "data-n", "2").unwrap();
    restyle(&mut doc, &cascade, &mut styles);
    assert_eq!(styles.generated()[0].content, "note: 2");
    assert_fresh(&doc, &cascade, &styles, Interaction::default());

    // A second box appears before the first, and the tree grows.
    let t = id(&doc, "t");
    doc.mutate().set_attribute(t, "class", "note").unwrap();
    let extra = doc.create_element("span");
    let far = id(&doc, "far");
    doc.mutate().append_child(far, extra).unwrap();
    restyle(&mut doc, &cascade, &mut styles);
    assert_eq!(styles.generated().len(), 2);
    assert_fresh(&doc, &cascade, &styles, Interaction::default());

    doc.mutate().remove_attribute(n, "class").unwrap();
    doc.mutate().remove_attribute(t, "class").unwrap();
    restyle(&mut doc, &cascade, &mut styles);
    assert!(styles.generated().is_empty());
    assert_fresh(&doc, &cascade, &styles, Interaction::default());

    // And one appears where there were none.
    doc.mutate().set_attribute(t, "class", "note").unwrap();
    restyle(&mut doc, &cascade, &mut styles);
    assert_eq!(styles.generated().len(), 1);
    assert_fresh(&doc, &cascade, &styles, Interaction::default());
}

#[test]
fn edits_inside_one_restyled_subtree_style_it_once() {
    let (mut doc, cascade) = page();
    let mut styles = cascade.compute(&doc, Interaction::default());
    let (a, b1) = (id(&doc, "a"), id(&doc, "b1"));
    doc.mutate().set_attribute(b1, "class", "on").unwrap();
    doc.mutate().set_attribute(a, "class", "on").unwrap();
    assert_eq!(restyle(&mut doc, &cascade, &mut styles), 5);
    assert_fresh(&doc, &cascade, &styles, Interaction::default());
}

#[test]
fn a_change_to_the_root_element_or_an_overflowed_log_restyles_everything() {
    let (mut doc, cascade) = page();
    let all = doc.elements().len();
    let mut styles = cascade.compute(&doc, Interaction::default());
    let html = doc.elements()[0];
    doc.mutate()
        .set_attribute(html, "style", "font-size: 10px")
        .unwrap();
    assert_eq!(restyle(&mut doc, &cascade, &mut styles), all);
    assert_fresh(&doc, &cascade, &styles, Interaction::default());

    doc.tree_mut();
    assert_eq!(restyle(&mut doc, &cascade, &mut styles), all);
}

#[test]
fn styles_from_another_cascade_are_recomputed() {
    let (mut doc, cascade) = page();
    let all = doc.elements().len();
    let other = Cascade::new(&[], Viewport::DEFAULT);
    let mut styles = other.compute(&doc, Interaction::default());
    let a = id(&doc, "a");
    doc.mutate().set_attribute(a, "class", "on").unwrap();
    assert_eq!(restyle(&mut doc, &cascade, &mut styles), all);
    assert_fresh(&doc, &cascade, &styles, Interaction::default());
}

#[test]
fn nothing_changed_restyles_nothing() {
    let (mut doc, cascade) = page();
    let mut styles = cascade.compute(&doc, Interaction::default());
    assert_eq!(restyle(&mut doc, &cascade, &mut styles), 0);
    // No selector reads text.
    let text = doc.tree().children(id(&doc, "b"))[0]; // beside an element
    doc.mutate().set_text(text, "changed").unwrap();
    assert_eq!(restyle(&mut doc, &cascade, &mut styles), 0);
    // A detached element's edits reach nothing in the document.
    let loose = doc.create_element("li");
    let inner = doc.create_element("b");
    doc.mutate().append_child(loose, inner).unwrap();
    doc.mutate().set_attribute(loose, "class", "on").unwrap();
    doc.mutate().set_attribute(inner, "class", "on").unwrap();
    assert_eq!(restyle(&mut doc, &cascade, &mut styles), 0);
}

#[test]
fn an_edit_under_an_unstyled_parent_restyles_everything() {
    // The log was dropped after `loose` went in, so it has no style for
    // its child to inherit.
    let (mut doc, cascade) = page();
    let mut styles = cascade.compute(&doc, Interaction::default());
    let loose = doc.create_element("li");
    let inner = doc.create_element("b");
    doc.mutate().append_child(loose, inner).unwrap();
    let list = id(&doc, "list");
    doc.mutate().append_child(list, loose).unwrap();
    doc.take_changes();
    doc.mutate().set_attribute(inner, "class", "on").unwrap();
    assert_eq!(
        restyle(&mut doc, &cascade, &mut styles),
        doc.elements().len()
    );
    assert_fresh(&doc, &cascade, &styles, Interaction::default());
}

#[test]
fn a_hover_change_restyles_the_nodes_it_moves_between() {
    let (doc, cascade) = page();
    let sheet = Stylesheet::parse(
        ":hover { color: orange } :focus { margin-top: 9px } :active { margin-bottom: 1px }",
    )
    .unwrap();
    let cascade = {
        let _ = cascade;
        Cascade::new(&[sheet], Viewport::DEFAULT)
    };
    let (a, c) = (id(&doc, "a"), id(&doc, "c"));
    let hover = |node| Interaction {
        hovered: Some(node),
        ..Interaction::default()
    };
    let mut styles = cascade.compute(&doc, hover(a));
    cascade.restyle_for(
        &mut styles,
        &doc,
        hover(c),
        [Change::Attributes(a), Change::Attributes(c)],
    );
    assert_fresh(&doc, &cascade, &styles, hover(c));

    // Styles remember what they were computed for, so the move alone is
    // enough: the old and the new hovered node are restyled.
    let t = id(&doc, "t");
    let restyled = cascade.restyle(&mut styles, &doc, hover(t), &Default::default());
    assert_eq!(restyled, 5 + 2, "the list's descendants, then #side's");
    assert_fresh(&doc, &cascade, &styles, hover(t));
    let focus = Interaction {
        focused: Some(a),
        ..hover(t)
    };
    let restyled = cascade.restyle(&mut styles, &doc, focus, &Default::default());
    assert_eq!(restyled, 5, "only the list: the hover stayed put");
    assert_fresh(&doc, &cascade, &styles, focus);
    let press = Interaction {
        active: Some(t),
        ..focus
    };
    cascade.restyle(&mut styles, &doc, press, &Default::default());
    assert_fresh(&doc, &cascade, &styles, press);
}
