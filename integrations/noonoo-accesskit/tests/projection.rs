//! Projection tests against AccessKit's protocol types.

use accesskit::{NodeId, Role, Toggled, TreeUpdate};
use noonoo_a11y::{A11yTree, build, build_in};
use noonoo_geometry::Size;
use noonoo_layout::MetricShaper;

fn tree(html: &str) -> (noonoo_dom::Document, A11yTree) {
    let doc = noonoo_html::parse(html);
    let styles = noonoo_style::compute(&doc, &[]);
    let tree = build(&doc, &styles);
    (doc, tree)
}

fn projected(update: &TreeUpdate, id: u64) -> &accesskit::Node {
    update
        .nodes
        .iter()
        .find(|(node_id, _)| node_id.0 == id)
        .map(|(_, node)| node)
        .expect("node is projected")
}

#[test]
fn projects_every_node_and_the_root() {
    let (_, tree) = tree("<h1>Hi</h1><p>Body <b>text</b></p>");
    let update = noonoo_accesskit::tree_update(&tree);

    assert_eq!(update.nodes.len(), tree.count());
    assert_eq!(update.tree.as_ref().unwrap().root, NodeId(tree.root.id));
    assert_eq!(update.focus, NodeId(tree.root.id));
}

#[test]
fn maps_roles_names_levels_and_states() {
    let (doc, tree) = tree(
        "<h2>Title</h2><button disabled>Save</button>\
         <input type=\"checkbox\" checked>\
         <a href=\"/\">Home</a>",
    );
    let update = noonoo_accesskit::tree_update(&tree);

    let heading_source = doc.get_elements_by_tag_name("h2")[0];
    let heading = projected(&update, tree.find(heading_source).unwrap().id);
    assert_eq!(heading.role(), Role::Heading);
    assert_eq!(heading.label(), Some("Title"));
    assert_eq!(heading.level(), Some(2));

    let button_source = doc.get_elements_by_tag_name("button")[0];
    let button = projected(&update, tree.find(button_source).unwrap().id);
    assert_eq!(button.role(), Role::Button);
    assert!(button.is_disabled());
    assert!(button.supports_action(accesskit::Action::Click));

    let checkbox_source = doc.get_elements_by_tag_name("input")[0];
    let checkbox = projected(&update, tree.find(checkbox_source).unwrap().id);
    assert_eq!(checkbox.role(), Role::CheckBox);
    assert_eq!(checkbox.toggled(), Some(Toggled::True));

    let link_source = doc.get_elements_by_tag_name("a")[0];
    let link = projected(&update, tree.find(link_source).unwrap().id);
    assert_eq!(link.role(), Role::Link);
    assert_eq!(link.label(), Some("Home"));
}

#[test]
fn projects_bounds_when_layout_is_present() {
    let doc =
        noonoo_html::parse("<div id=\"box\" style=\"width:100px;height:20px;margin:0\">box</div>");
    let styles = noonoo_style::compute(&doc, &[]);
    let viewport = Size::new(800.0, 600.0);
    let layout = noonoo_layout::layout(&doc, &styles, viewport, &MetricShaper);
    let tree = build_in(&doc, &styles, Some(&layout), None);
    let update = noonoo_accesskit::tree_update(&tree);

    let source = doc.get_element_by_id("box").unwrap();
    let node = projected(&update, tree.find(source).unwrap().id);
    let bounds = node.bounds().expect("bounds are projected");
    assert_eq!(bounds.x1 - bounds.x0, 100.0);
    assert_eq!(bounds.y1 - bounds.y0, 20.0);
}

#[test]
fn focused_node_reaches_the_update_and_falls_back_to_root() {
    let doc = noonoo_html::parse("<button id=\"b\">Save</button>");
    let styles = noonoo_style::compute(&doc, &[]);
    let button = doc.get_element_by_id("b").unwrap();

    let focused = build_in(&doc, &styles, None, Some(button));
    let update = noonoo_accesskit::tree_update(&focused);
    assert_eq!(update.focus.0, focused.focus.unwrap());

    let unfocused = build(&doc, &styles);
    let update = noonoo_accesskit::tree_update(&unfocused);
    assert_eq!(update.focus.0, unfocused.root.id);
}
