//! Robustness: pathological but *bounded* inputs must never panic, hang, or
//! throw away the tree.
//!
//! This is the stable-CI counterpart to `fuzz/`'s nightly targets. The corpus
//! and the pseudo-random input are deterministic (fixed seed), so a failure
//! reproduces exactly.

use noonoo_html::parse;

/// Depth of the tallest pathological tree the runner builds. The parser and
/// `text_content` are iterative/safe well past this.
const NESTED_DEPTH: usize = 2048;

#[test]
fn adversarial_corpus_never_panics_and_keeps_a_tree() {
    let cases = [
        "",
        "<",
        "<div",
        "<div x=",
        "<div x=\"unterminated",
        "<div <!-- comment",
        "</div>",
        "<<>>",
        "<script>",
        "<script>unterminated",
        "<style>unterminated",
        "<textarea>unterminated",
        "<title>unterminated",
        "<!--unterminated",
        "<!doctype",
        "<!doctype html><html@weird><body>ok</body></html>",
        "\0\x01\x02",
        "<p>a</p\n<b>",
        "<div class=<a>>x",
        "<p>😀oggled</p>",
        "\r\n\r\n  \t<crlf>\r\n</crlf>\r\n",
        "&amp;&nbsp;&#x1F600;&#;",
        "<br><!--[if IE]><p>no</p><![endif]--><br clear=all>",
    ];

    for case in cases {
        let doc = parse(case);
        let root = doc.root();
        assert!(doc.tree().contains(root), "the tree always has a root");
        // Walk what the parser produced. Reads are total functions.
        let elements = doc.elements();
        for &id in elements.iter().take(64) {
            let _ = doc.text_content(id);
        }
    }
}

#[test]
fn pseudo_random_input_never_panics() {
    let mut rng = Rng::new(0x2026_1107);
    let alphabet: &[char] = &[
        'a', '<', '>', '/', '"', '\'', '=', ';', '\n', '\t', '&', '{', '}', '\\', 'é', '日',
    ];
    for _ in 0..2000 {
        let length = rng.next() as usize % 300;
        let source = random_string(&mut rng, length, alphabet);
        let doc = parse(&source);
        let _ = doc.text_content(doc.root());
        let _ = doc.elements().len();
    }
}

#[test]
fn random_bytes_lossily_decoded_never_panic() {
    let mut rng = Rng::new(0x2026_1107);
    for _ in 0..2000 {
        let mut bytes = Vec::new();
        for _ in 0..(rng.next() as usize % 200) {
            bytes.push(rng.next() as u8);
        }
        let source = String::from_utf8_lossy(&bytes);
        let doc = parse(&source);
        let _ = doc.elements().len();
    }
}

#[test]
fn deep_but_bounded_nesting_stays_finite() {
    let open = "<div style=\"x\">".repeat(NESTED_DEPTH);
    let close = "</div>".repeat(NESTED_DEPTH);
    let doc = parse(&format!("{open}{close}"));
    // Plus the html, head and body every document has.
    assert_eq!(doc.elements().len(), NESTED_DEPTH + 3);
}

// --- the deterministic generator -------------------------------------------

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(0x5851_F42D_4C95_7F2D).wrapping_add(1);
        self.0.rotate_right(33)
    }
}

fn random_string(rng: &mut Rng, length: usize, alphabet: &[char]) -> String {
    (0..length)
        .map(|_| alphabet[rng.next() as usize % alphabet.len()])
        .collect()
}

/// Non-ASCII whitespace inside a tag is skipped by whole characters, not by
/// bytes: U+2006 and U+2028 are three bytes each.
#[test]
fn multibyte_whitespace_inside_a_tag_does_not_panic() {
    for source in [
        "<ht\u{2006}ml>><h1>hello</h1",
        "<a \u{2028}]a>x",
        "<a b\u{2028}=c>x",
        "<a b=\u{2028}c>x",
        "<p\u{3000}id=x>y</p>",
    ] {
        let doc = noonoo_html::parse(source);
        let _ = noonoo_html::serialize::inner_html(&doc, doc.root());
    }
}

/// An attribute value after multi-byte whitespace is still read.
#[test]
fn an_attribute_after_an_ideographic_space_is_kept() {
    let doc = noonoo_html::parse("<p\u{3000}id=x>y</p>");
    let html = noonoo_html::serialize::inner_html(&doc, doc.root());
    assert!(html.contains(r#"id="x""#), "{html}");
}

/// The tree is at most `MAX_TREE_DEPTH` deep, as in Chromium's parser:
/// past it an element goes to the current node's parent, so nesting turns
/// into siblings and nothing is lost. Recursive style, layout and paint can
/// then never be driven past a fixed depth by markup.
#[test]
fn nesting_past_the_depth_limit_becomes_siblings() {
    let n = 3000;
    let doc = parse(&format!("{}x{}", "<div>".repeat(n), "</div>".repeat(n)));
    let tree = doc.tree();
    let mut deepest = 0;
    let mut divs = 0;
    let mut stack = vec![(doc.root(), 0usize)];
    while let Some((node, depth)) = stack.pop() {
        deepest = deepest.max(depth);
        if doc.element(node).is_some_and(|e| e.name == "div") {
            divs += 1;
        }
        for child in tree.children(node) {
            stack.push((*child, depth + 1));
        }
    }
    assert!(
        deepest <= noonoo_html::MAX_TREE_DEPTH + 1,
        "depth {deepest}"
    );
    assert_eq!(divs, n, "every element is kept");
    assert!(noonoo_html::serialize::inner_html(&doc, doc.root()).contains('x'));
}

/// Under the limit, nesting is untouched.
#[test]
fn nesting_under_the_depth_limit_is_kept() {
    let doc = parse("<div><div><p>x</p></div></div>");
    assert_eq!(
        noonoo_html::serialize::inner_html(&doc, doc.get_elements_by_tag_name("body")[0]),
        "<div><div><p>x</p></div></div>"
    );
}

/// The deepest node, the document being 0.
fn depth(doc: &noonoo_dom::Document) -> usize {
    let mut deepest = 0;
    let mut stack = vec![(doc.root(), 0)];
    while let Some((node, depth)) = stack.pop() {
        deepest = deepest.max(depth);
        stack.extend(doc.tree().children(node).iter().map(|&c| (c, depth + 1)));
    }
    deepest
}

/// Misnested markup the tree builder moves and copies: however much of it
/// there is, the tree stays within the depth limit and in proportion to the
/// source.
#[test]
fn misnesting_stays_bounded() {
    const N: usize = 4000;
    let numbered = |tag: &str| -> String { (0..N).map(|i| format!("<{tag} a={i}>")).collect() };
    for source in [
        "<a><div>".repeat(N),
        "<b><p></b>".repeat(N),
        "<b>".repeat(N) + "<p>" + &"</b>".repeat(N),
        format!("<b>{}<p>{}", "<i>".repeat(N), "</b>".repeat(N)),
        numbered("b") + &"<p>x".repeat(N),
        numbered("font") + &"<div></font>".repeat(N),
        format!("<table>{}", "a<b>".repeat(N)),
        "<table><b><tr><td>".repeat(N),
        "<table><a>".repeat(N),
        "<svg><desc><table><b>".repeat(N),
        "<template><table><tr><a>".repeat(N),
        "<button><b><p>".repeat(N),
        "<nobr><div>".repeat(N),
        "<div>".repeat(N) + &"</x>".repeat(N),
    ] {
        let doc = parse(&source);
        assert!(depth(&doc) <= noonoo_html::MAX_TREE_DEPTH, "{source:.40}");
        // The copies' allowance, in elements (see `builder`).
        assert!(
            doc.elements().len() < 270_000 + source.len(),
            "{source:.40}"
        );
    }
}

/// Tags that nest, close, move and copy, in no order: every parse gives a
/// tree whose links agree, the same one each time.
#[test]
fn random_tags_give_a_consistent_tree() {
    const TAGS: &[&str] = &[
        "a",
        "b",
        "i",
        "nobr",
        "font",
        "p",
        "div",
        "li",
        "ul",
        "dd",
        "h1",
        "table",
        "tbody",
        "tr",
        "td",
        "caption",
        "colgroup",
        "col",
        "select",
        "option",
        "optgroup",
        "input",
        "button",
        "form",
        "template",
        "svg",
        "title",
        "desc",
        "foreignObject",
        "math",
        "mi",
        "annotation-xml",
        "frameset",
        "html",
        "head",
        "body",
        "applet",
        "ruby",
        "rt",
        "textarea",
        "script",
        "style",
        "noscript",
        "br",
        "hr",
        "image",
        "pre",
        "x-y",
    ];
    let mut rng = Rng::new(7);
    for round in 0..1500 {
        let mut source = String::new();
        for _ in 0..(rng.next() as usize % 120) {
            let tag = TAGS[rng.next() as usize % TAGS.len()];
            match rng.next() % 8 {
                0..=3 => source += &format!("<{tag} a={}>", rng.next() % 4),
                4 | 5 => source += &format!("</{tag}>"),
                6 => source += "x",
                _ => {
                    source += [" ", "<!--c-->", "<![CDATA[d]]>", "&amp", "\0", "<b>"]
                        [rng.next() as usize % 6]
                }
            }
        }
        let doc = match round % 3 {
            0 => {
                let mut doc = noonoo_dom::Document::new();
                let tag = TAGS[rng.next() as usize % TAGS.len()];
                let root = doc.root();
                let node = doc.create_element(tag);
                doc.mutate().append_child(root, node).unwrap();
                noonoo_html::set_inner_html(&mut doc, node, &source).unwrap();
                doc
            }
            _ => parse(&source),
        };
        let tree = doc.tree();
        let nodes = tree.descendants(doc.root());
        let mut seen = std::collections::HashSet::new();
        for &node in &nodes {
            assert!(seen.insert(node), "{source}");
            for &child in tree.children(node) {
                assert_eq!(tree.parent(child), Some(node), "{source}");
            }
            // Text is joined up.
            let texts = tree.children(node).windows(2).filter(|pair| {
                pair.iter()
                    .all(|&id| doc.node(id).is_some_and(|n| n.as_text().is_some()))
            });
            let moved = source.contains("</") || source.contains("<a") || source.contains("<nobr");
            assert!(moved || texts.count() == 0, "{source}");
        }
        if round % 3 != 0 {
            let html = tree.children(doc.root()).iter();
            let elements = html.filter(|&&id| doc.element(id).is_some()).count();
            assert_eq!(elements, 1, "{source}");
        }
    }
}
