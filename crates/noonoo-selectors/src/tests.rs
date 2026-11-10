use super::*;
use noonoo_dom::{Document, NodeId};

#[derive(Clone, Copy)]
struct View<'a> {
    doc: &'a Document,
    id: NodeId,
}

impl<'a> View<'a> {
    fn element_children(&self, id: NodeId) -> Vec<NodeId> {
        self.doc
            .tree()
            .children(id)
            .iter()
            .copied()
            .filter(|&c| self.doc.element(c).is_some())
            .collect()
    }
}

impl<'a> Element for View<'a> {
    fn tag_name(&self) -> String {
        self.doc
            .element(self.id)
            .map(|e| e.name.clone())
            .unwrap_or_default()
    }
    fn id(&self) -> Option<String> {
        self.doc
            .element(self.id)
            .and_then(|e| e.id().map(str::to_owned))
    }
    fn classes(&self) -> Vec<String> {
        self.doc
            .element(self.id)
            .map(|e| e.classes().map(str::to_owned).collect())
            .unwrap_or_default()
    }
    fn attr(&self, name: &str) -> Option<String> {
        self.doc
            .element(self.id)
            .and_then(|e| e.attr(name).map(str::to_owned))
    }
    fn parent(&self) -> Option<Self> {
        let parent = self.doc.tree().parent(self.id)?;
        self.doc.element(parent).is_some().then_some(View {
            doc: self.doc,
            id: parent,
        })
    }
    fn previous_siblings(&self) -> Vec<Self> {
        let Some(parent) = self.doc.tree().parent(self.id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for child in self.element_children(parent) {
            if child == self.id {
                break;
            }
            out.push(View {
                doc: self.doc,
                id: child,
            });
        }
        out
    }
    fn sibling_index(&self) -> usize {
        let Some(parent) = self.doc.tree().parent(self.id) else {
            return 0;
        };
        self.element_children(parent)
            .iter()
            .position(|&c| c == self.id)
            .unwrap_or(0)
    }
    fn sibling_count(&self) -> usize {
        let Some(parent) = self.doc.tree().parent(self.id) else {
            return 1;
        };
        self.element_children(parent).len()
    }
}

fn sample() -> (Document, NodeId, NodeId) {
    // body > ul#items > li.a, li.b, li.c
    let mut doc = Document::new();
    let root = doc.root();
    let body = doc.create_element("body");
    doc.mutate().append_child(root, body).unwrap();
    let ul = doc.create_element("ul");
    doc.mutate().set_attribute(ul, "id", "items").unwrap();
    doc.mutate().append_child(body, ul).unwrap();
    for class in ["a", "b", "c"] {
        let li = doc.create_element("li");
        doc.mutate().set_attribute(li, "class", class).unwrap();
        doc.mutate().append_child(ul, li).unwrap();
    }
    (doc, body, ul)
}

fn view<'a>(doc: &'a Document, id: NodeId) -> View<'a> {
    View { doc, id }
}

fn ctx() -> MatchContext {
    MatchContext::default()
}

#[test]
fn parses_and_matches_type_class_id() {
    let (doc, _body, ul) = sample();
    let s = SelectorList::parse("ul#items").unwrap();
    assert!(s.matches(&view(&doc, ul), ctx()));
    let li = doc.get_elements_by_tag_name("li")[1];
    assert!(
        SelectorList::parse("li.b")
            .unwrap()
            .matches(&view(&doc, li), ctx())
    );
    assert!(
        !SelectorList::parse("li.a")
            .unwrap()
            .matches(&view(&doc, li), ctx())
    );
}

#[test]
fn child_and_descendant_combinators() {
    let (doc, _body, ul) = sample();
    let li = doc.get_elements_by_tag_name("li")[0];
    assert!(
        SelectorList::parse("body ul li")
            .unwrap()
            .matches(&view(&doc, li), ctx())
    );
    assert!(
        SelectorList::parse("ul > li")
            .unwrap()
            .matches(&view(&doc, li), ctx())
    );
    // No direct child relationship between body and li.
    assert!(
        !SelectorList::parse("body > li")
            .unwrap()
            .matches(&view(&doc, li), ctx())
    );
    assert!(
        SelectorList::parse("ul")
            .unwrap()
            .matches(&view(&doc, ul), ctx())
    );
}

#[test]
fn sibling_combinators() {
    let (doc, _body, _ul) = sample();
    let lis = doc.get_elements_by_tag_name("li");
    // li.a + li.b matches li.b
    assert!(
        SelectorList::parse("li.a + li.b")
            .unwrap()
            .matches(&view(&doc, lis[1]), ctx())
    );
    // li.a ~ li.c matches li.c
    assert!(
        SelectorList::parse("li.a ~ li.c")
            .unwrap()
            .matches(&view(&doc, lis[2]), ctx())
    );
    assert!(
        !SelectorList::parse("li.c + li.a")
            .unwrap()
            .matches(&view(&doc, lis[0]), ctx())
    );
}

#[test]
fn structural_pseudo_classes() {
    let (doc, _body, _ul) = sample();
    let lis = doc.get_elements_by_tag_name("li");
    assert!(
        SelectorList::parse("li:first-child")
            .unwrap()
            .matches(&view(&doc, lis[0]), ctx())
    );
    assert!(
        SelectorList::parse("li:last-child")
            .unwrap()
            .matches(&view(&doc, lis[2]), ctx())
    );
    assert!(
        !SelectorList::parse("li:first-child")
            .unwrap()
            .matches(&view(&doc, lis[1]), ctx())
    );
    assert!(
        SelectorList::parse("li:nth-child(2)")
            .unwrap()
            .matches(&view(&doc, lis[1]), ctx())
    );
    assert!(
        SelectorList::parse("li:nth-child(odd)")
            .unwrap()
            .matches(&view(&doc, lis[0]), ctx())
    );
    assert!(
        SelectorList::parse("li:nth-child(2n+1)")
            .unwrap()
            .matches(&view(&doc, lis[2]), ctx())
    );
}

#[test]
fn logical_pseudo_classes() {
    let (doc, _body, _ul) = sample();
    let lis = doc.get_elements_by_tag_name("li");
    assert!(
        SelectorList::parse("li:not(.a)")
            .unwrap()
            .matches(&view(&doc, lis[1]), ctx())
    );
    assert!(
        SelectorList::parse("li:is(.a, .c)")
            .unwrap()
            .matches(&view(&doc, lis[2]), ctx())
    );
}

#[test]
fn attribute_selectors() {
    let mut doc = Document::new();
    let root = doc.root();
    let a = doc.create_element("a");
    doc.mutate()
        .set_attribute(a, "href", "https://example.com/x")
        .unwrap();
    doc.mutate()
        .set_attribute(a, "data-role", "nav item")
        .unwrap();
    doc.mutate().append_child(root, a).unwrap();
    let v = view(&doc, a);

    assert!(SelectorList::parse("[href]").unwrap().matches(&v, ctx()));
    assert!(
        SelectorList::parse("a[data-role~=nav]")
            .unwrap()
            .matches(&v, ctx())
    );
    assert!(
        SelectorList::parse("a[href^=https]")
            .unwrap()
            .matches(&v, ctx())
    );
    assert!(
        SelectorList::parse("a[href$=x]")
            .unwrap()
            .matches(&v, ctx())
    );
    assert!(
        SelectorList::parse("a[href*=example]")
            .unwrap()
            .matches(&v, ctx())
    );
    assert!(
        !SelectorList::parse("[lang|=en]")
            .unwrap()
            .matches(&v, ctx())
    );
}

#[test]
fn dynamic_pseudo_classes_use_context() {
    let (doc, _body, _ul) = sample();
    let li = doc.get_elements_by_tag_name("li")[0];
    let hovered = MatchContext {
        hovered: true,
        ..MatchContext::default()
    };
    assert!(
        SelectorList::parse("li:hover")
            .unwrap()
            .matches(&view(&doc, li), hovered)
    );
    assert!(
        !SelectorList::parse("li:hover")
            .unwrap()
            .matches(&view(&doc, li), ctx())
    );
}

#[test]
fn specificity_is_computed() {
    let spec = |s: &str| SelectorList::parse(s).unwrap().max_specificity().to_u32();
    assert!(spec("#id") > spec(".cls"));
    assert!(spec(".cls") > spec("div"));
    assert!(spec("ul li") > spec("li"));
    assert_eq!(spec("*"), 0);
    assert_eq!(
        spec("li.a"),
        Specificity {
            ids: 0,
            classes: 1,
            types: 1
        }
        .to_u32()
    );
    // :not() takes its argument's specificity.
    assert_eq!(
        spec("li:not(.a)"),
        Specificity {
            ids: 0,
            classes: 1,
            types: 1
        }
        .to_u32()
    );
}

#[test]
fn pseudo_element_never_matches_an_element() {
    let (doc, _body, ul) = sample();
    assert!(
        !SelectorList::parse("ul::before")
            .unwrap()
            .matches(&view(&doc, ul), ctx())
    );
}

#[test]
fn unknown_pseudo_class_parses_but_never_matches() {
    let (doc, _body, ul) = sample();
    let s = SelectorList::parse("ul:defined").unwrap();
    assert!(!s.matches(&view(&doc, ul), ctx()));
}

#[test]
fn parse_errors_are_reported() {
    assert!(SelectorList::parse("").is_err());
    assert!(SelectorList::parse("li >").is_err());
    assert!(SelectorList::parse("[href").is_err());
}

#[test]
fn a_pseudo_element_selector_matches_for_its_originating_element() {
    let (doc, body, ul) = sample();
    let ul = view(&doc, ul);
    let list = SelectorList::parse("body > ul::before, #items::after, li::before").unwrap();
    // `ul::before` has specificity (0,0,2+1): type + pseudo-element.
    assert_eq!(
        list.pseudo_matching_specificity(&ul, ctx(), "before"),
        Some(Specificity {
            ids: 0,
            classes: 0,
            types: 3
        })
    );
    assert_eq!(
        list.pseudo_matching_specificity(&ul, ctx(), "AFTER"),
        Some(Specificity {
            ids: 1,
            classes: 0,
            types: 1
        })
    );
    assert_eq!(list.pseudo_matching_specificity(&ul, ctx(), "marker"), None);
    assert_eq!(
        list.pseudo_matching_specificity(&view(&doc, body), ctx(), "before"),
        None
    );
    // A selector without a pseudo-element doesn't style one.
    let plain = SelectorList::parse("ul").unwrap();
    assert_eq!(
        plain.pseudo_matching_specificity(&ul, ctx(), "before"),
        None
    );
}

#[test]
fn a_pseudo_element_only_counts_on_the_last_compound() {
    let (doc, _body, ul) = sample();
    let li = doc.get_elements_by_tag_name("li")[0];
    let s = SelectorList::parse("ul::before li").unwrap();
    assert_eq!(
        s.pseudo_matching_specificity(&view(&doc, li), ctx(), "before"),
        None
    );
    assert!(!s.matches(&view(&doc, li), ctx()));
    let _ = ul;
}

#[test]
fn legacy_single_colon_before_and_after_are_pseudo_elements() {
    let (doc, _body, ul) = sample();
    let ul = view(&doc, ul);
    let s = SelectorList::parse("ul:before").unwrap();
    assert!(!s.matches(&ul, ctx()));
    assert!(
        s.pseudo_matching_specificity(&ul, ctx(), "before")
            .is_some()
    );
    assert!(
        SelectorList::parse("ul:after")
            .unwrap()
            .pseudo_matching_specificity(&ul, ctx(), "after")
            .is_some()
    );
}

#[test]
fn dynamic_state_applies_to_the_originating_element() {
    let (doc, _body, ul) = sample();
    let ul = view(&doc, ul);
    let s = SelectorList::parse("ul:hover::after").unwrap();
    assert!(s.pseudo_matching_specificity(&ul, ctx(), "after").is_none());
    let hovered = MatchContext {
        hovered: true,
        ..MatchContext::default()
    };
    assert!(
        s.pseudo_matching_specificity(&ul, hovered, "after")
            .is_some()
    );
}

/// HTML compares some attributes' values without ASCII case in selectors
/// (`type`, `align`, `rel`, ...): `<input type=RADIO>` is
/// `input[type=radio]`. Other attributes keep their case.
#[test]
fn some_html_attributes_match_without_case() {
    let mut doc = Document::new();
    let root = doc.root();
    let input = doc.create_element("input");
    doc.mutate().set_attribute(input, "type", "RADIO").unwrap();
    doc.mutate()
        .set_attribute(input, "data-kind", "Big")
        .unwrap();
    doc.mutate().append_child(root, input).unwrap();
    let v = view(&doc, input);
    let matches = |selector: &str| SelectorList::parse(selector).unwrap().matches(&v, ctx());
    assert!(matches("input[type=radio]"));
    assert!(matches("[type^=rad]"));
    assert!(!matches("[data-kind=big]"));
    assert!(matches("[data-kind=Big]"));
}
