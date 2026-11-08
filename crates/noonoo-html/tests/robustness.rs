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
    assert_eq!(doc.elements().len(), NESTED_DEPTH);
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
