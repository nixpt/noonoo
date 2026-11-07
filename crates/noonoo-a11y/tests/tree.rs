//! Behavioural tests for the accessibility tree.

use noonoo_a11y::{A11yNode, A11yTree, Checked, Role, build, build_in, compose};
use noonoo_dom::{Document, NodeId};
use noonoo_geometry::{Point, Size};
use noonoo_layout::MetricShaper;

fn parse(html: &str) -> (Document, A11yTree) {
    let doc = noonoo_html::parse(html);
    let styles = noonoo_style::compute(&doc, &[]);
    let tree = build(&doc, &styles);
    (doc, tree)
}

fn find_role(node: &A11yNode, role: Role) -> Option<&A11yNode> {
    if node.role == role {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_role(child, role))
}

fn find_name<'a>(node: &'a A11yNode, name: &str) -> Option<&'a A11yNode> {
    if node.name == name {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_name(child, name))
}

fn by_id<'a>(doc: &Document, tree: &'a A11yTree, id: &str) -> Option<&'a A11yNode> {
    tree.find(doc.get_element_by_id(id)?)
}

fn state_ids(node: &A11yNode, out: &mut Vec<u64>) {
    out.push(node.id);
    for child in &node.children {
        state_ids(child, out);
    }
}

#[test]
fn maps_landmarks_headings_and_links() {
    let (_, tree) = parse(
        "<html><head><title>Demo</title></head><body>\
         <nav><a href=\"/\">Home</a></nav>\
         <main><h1>Title</h1><p>Body</p></main>\
         <footer>End</footer></body></html>",
    );

    assert_eq!(tree.root.role, Role::Document);
    assert_eq!(tree.root.name, "Demo");
    assert!(find_role(&tree.root, Role::Navigation).is_some());
    assert!(find_role(&tree.root, Role::Main).is_some());

    let heading = find_role(&tree.root, Role::Heading).unwrap();
    assert_eq!(heading.name, "Title");
    assert_eq!(heading.level, Some(1));

    let link = find_role(&tree.root, Role::Link).unwrap();
    assert_eq!(link.name, "Home");

    assert!(find_role(&tree.root, Role::Paragraph).is_some());
    assert!(find_role(&tree.root, Role::ContentInfo).is_some());
}

#[test]
fn computes_names_from_aria_labels_and_content() {
    let (doc, tree) = parse(
        "<input id=\"q\" type=\"text\" aria-label=\"Search\">\
         <span id=\"lbl\">Email</span><input id=\"e\" aria-labelledby=\"lbl\">\
         <label for=\"p\">Password</label><input id=\"p\" type=\"password\">\
         <img id=\"i\" src=\"x.png\" alt=\"A cat\">\
         <button id=\"b\">Save</button>\
         <div id=\"t\" title=\"Note\"></div>",
    );

    assert_eq!(by_id(&doc, &tree, "q").unwrap().name, "Search");
    assert_eq!(by_id(&doc, &tree, "e").unwrap().name, "Email");
    assert_eq!(by_id(&doc, &tree, "p").unwrap().name, "Password");
    assert_eq!(by_id(&doc, &tree, "i").unwrap().name, "A cat");
    assert_eq!(by_id(&doc, &tree, "b").unwrap().name, "Save");
    assert_eq!(by_id(&doc, &tree, "t").unwrap().name, "Note");
}

#[test]
fn computes_descriptions_from_aria_title_and_placeholder() {
    let (doc, tree) = parse(
        "<span id=\"hint\">Must be at least 8 characters</span>\
         <input id=\"a\" aria-describedby=\"hint\">\
         <input id=\"b\" placeholder=\"Search terms\">\
         <button id=\"c\" title=\"Saves the page\">Save</button>",
    );

    assert_eq!(
        by_id(&doc, &tree, "a").unwrap().description,
        "Must be at least 8 characters"
    );
    assert_eq!(by_id(&doc, &tree, "b").unwrap().description, "Search terms");
    assert_eq!(
        by_id(&doc, &tree, "c").unwrap().description,
        "Saves the page"
    );
}

#[test]
fn hidden_content_is_absent_but_presentational_children_are_promoted() {
    let (doc, tree) = parse(
        "<div id=\"gone\" style=\"display:none\"><button>Gone</button></div>\
         <div id=\"aria\" aria-hidden=\"true\"><button>Aria</button></div>\
         <div id=\"attr\" hidden><button>Attr</button></div>\
         <div id=\"role\" role=\"presentation\"><button id=\"kept\">Kept</button></div>\
         <img id=\"decor\" src=\"x.png\" alt=\"\">",
    );

    assert!(by_id(&doc, &tree, "gone").is_none());
    assert!(by_id(&doc, &tree, "aria").is_none());
    assert!(by_id(&doc, &tree, "attr").is_none());
    assert!(by_id(&doc, &tree, "role").is_none());
    assert!(by_id(&doc, &tree, "decor").is_none());
    assert_eq!(by_id(&doc, &tree, "kept").unwrap().name, "Kept");
}

#[test]
fn visibility_hidden_drops_inherited_text_but_keeps_restored_children() {
    let (doc, tree) = parse(
        "<div id=\"hidden\" style=\"visibility:hidden\">\
           <span>inherited text</span>\
           <span id=\"shown\" style=\"visibility:visible\">shown text</span>\
         </div>",
    );

    assert!(by_id(&doc, &tree, "hidden").is_none());
    assert!(by_id(&doc, &tree, "shown").is_some());
    assert!(find_name(&tree.root, "inherited text").is_none());
    assert!(find_name(&tree.root, "shown text").is_some());
}

#[test]
fn text_runs_become_static_text() {
    let (_, tree) = parse("<p>Hello <b>world</b></p>");

    let paragraph = find_role(&tree.root, Role::Paragraph).unwrap();
    assert_eq!(paragraph.name, "");
    let text: Vec<&str> = paragraph
        .children
        .iter()
        .filter(|child| child.role == Role::StaticText)
        .map(|child| child.name.as_str())
        .collect();
    assert_eq!(text, ["Hello"]);

    let world = find_name(&tree.root, "world").unwrap();
    assert_eq!(world.role, Role::StaticText);
}

#[test]
fn computes_control_states() {
    let (doc, tree) = parse(
        "<input id=\"a\" type=\"checkbox\" checked disabled>\
         <input id=\"b\" type=\"checkbox\" aria-checked=\"mixed\">\
         <input id=\"c\" type=\"radio\" checked>\
         <input id=\"d\" required readonly>\
         <button id=\"e\" aria-expanded=\"true\">Menu</button>\
         <div id=\"f\" aria-busy=\"true\" aria-selected=\"true\"></div>",
    );

    let a = by_id(&doc, &tree, "a").unwrap();
    assert!(a.states.disabled);
    assert_eq!(a.states.checked, Some(Checked::True));

    assert_eq!(
        by_id(&doc, &tree, "b").unwrap().states.checked,
        Some(Checked::Mixed)
    );
    assert_eq!(
        by_id(&doc, &tree, "c").unwrap().states.checked,
        Some(Checked::True)
    );

    let d = by_id(&doc, &tree, "d").unwrap();
    assert!(d.states.required);
    assert!(d.states.read_only);

    assert_eq!(by_id(&doc, &tree, "e").unwrap().states.expanded, Some(true));

    let f = by_id(&doc, &tree, "f").unwrap();
    assert!(f.states.selected);
    assert!(f.states.busy);
}

#[test]
fn focus_marks_the_node_and_tree() {
    let doc = noonoo_html::parse("<button id=\"b\">Save</button>");
    let styles = noonoo_style::compute(&doc, &[]);
    let button = doc.get_element_by_id("b").unwrap();
    let tree = build_in(&doc, &styles, None, Some(button));

    let node = tree.find(button).unwrap();
    assert!(node.states.focused);
    assert_eq!(tree.focus, Some(node.id));
}

#[test]
fn bounds_come_from_layout() {
    let doc =
        noonoo_html::parse("<div id=\"box\" style=\"width:100px;height:20px;margin:0\">box</div>");
    let styles = noonoo_style::compute(&doc, &[]);
    let viewport = Size::new(800.0, 600.0);
    let layout = noonoo_layout::layout(&doc, &styles, viewport, &MetricShaper);
    let tree = build_in(&doc, &styles, Some(&layout), None);

    let node = tree.find(doc.get_element_by_id("box").unwrap()).unwrap();
    let bounds = node.bounds.unwrap();
    assert_eq!(bounds.width, 100.0);
    assert_eq!(bounds.height, 20.0);
}

#[test]
fn values_come_from_fields_and_selected_options() {
    let (doc, tree) = parse(
        "<input id=\"a\" value=\"Ada\">\
         <textarea id=\"b\">hello there</textarea>\
         <select id=\"c\"><option>One</option><option selected>Two</option></select>",
    );

    assert_eq!(
        by_id(&doc, &tree, "a").unwrap().value.as_deref(),
        Some("Ada")
    );
    assert_eq!(
        by_id(&doc, &tree, "b").unwrap().value.as_deref(),
        Some("hello there")
    );
    assert_eq!(
        by_id(&doc, &tree, "c").unwrap().value.as_deref(),
        Some("Two")
    );
}

#[test]
fn compose_offsets_renumbers_and_keeps_focus() {
    let doc_a = noonoo_html::parse("<button id=\"a\">A</button>");
    let styles_a = noonoo_style::compute(&doc_a, &[]);
    let focus = doc_a.get_element_by_id("a").unwrap();
    let a = build_in(&doc_a, &styles_a, None, Some(focus));

    let doc_b = noonoo_html::parse("<main>B</main>");
    let styles_b = noonoo_style::compute(&doc_b, &[]);
    let b = build(&doc_b, &styles_b);

    let window = compose("Window", &[(&a, Point::new(10.0, 20.0)), (&b, Point::ZERO)]);

    assert_eq!(window.root.role, Role::Window);
    assert_eq!(window.root.name, "Window");
    assert_eq!(window.root.children.len(), 2);
    assert_eq!(
        window.find(focus).unwrap().bounds,
        a.find(focus)
            .unwrap()
            .bounds
            .map(|rect| noonoo_geometry::Rect::new(
                rect.x + 10.0,
                rect.y + 20.0,
                rect.width,
                rect.height
            ))
    );

    let mut ids = Vec::new();
    state_ids(&window.root, &mut ids);
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), window.count());

    let focused = window.focus.unwrap();
    assert_eq!(window.find_by_id(focused).unwrap().name, "A");
    assert_eq!(window.find_by_id(focused).unwrap().layer, Some(0));

    let main = window
        .root
        .children
        .iter()
        .find(|child| child.layer == Some(1))
        .and_then(|child| child.children.first())
        .unwrap();
    assert_eq!(main.role, Role::Main);
    assert_eq!(main.layer, Some(1));
}

#[test]
fn explicit_aria_roles_win() {
    let (doc, tree) = parse("<div id=\"x\" role=\"tab\">Tab one</div>");

    let node = by_id(&doc, &tree, "x").unwrap();
    assert_eq!(node.role, Role::Tab);
    assert_eq!(node.name, "Tab one");
}

#[test]
fn excluded_tags_never_appear() {
    let (_, tree) = parse(
        "<html><head><meta charset=\"utf-8\"><style>p{color:red}</style></head>\
         <body><script>var x = 1;</script><template><p>T</p></template><p>Real</p></body></html>",
    );

    assert!(find_name(&tree.root, "var x = 1;").is_none());
    assert!(find_name(&tree.root, "T").is_none());
    assert!(find_name(&tree.root, "Real").is_some());
}

#[test]
fn ids_are_unique_and_dense_from_one() {
    let (_, tree) = parse("<h1>Hi</h1><p>One <b>two</b></p>");
    let mut ids = Vec::new();
    state_ids(&tree.root, &mut ids);
    ids.sort_unstable();
    assert_eq!(ids, (1..=tree.count() as u64).collect::<Vec<_>>());
}

#[test]
fn node_ids_are_source_ids() {
    let (doc, tree) = parse("<h1 id=\"h\">Hi</h1>");
    let source: NodeId = doc.get_element_by_id("h").unwrap();
    assert_eq!(tree.find(source).unwrap().role, Role::Heading);
}
