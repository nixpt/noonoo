//! Robustness: pathological but *bounded* stylesheets must either parse
//! cleanly or error out — never panic, and never overflow the stack.
//!
//! This is the stable-CI counterpart to `fuzz/`'s nightly targets. The corpus
//! and the pseudo-random input are deterministic (fixed seed), so a failure
//! reproduces exactly.

use noonoo_css::Stylesheet;

#[test]
fn adversarial_corpus_never_panics() {
    let cases = [
        "",
        "{",
        "}",
        "a",
        "a {",
        "a { color",
        "a { color:",
        "a { color: ; }",
        "a { color: url( }",
        "a { width: calc( }",
        "a { width: calc(calc() }",
        "a { margin: calc(1px + ) }",
        "@media { a { color: blue }",
        "@media screen { a { color",
        "@font-face { font-family A }",
        "@keyframes spin {",
        "a { --x: ; }",
        "a { margin: 1px 2px 3px 4px 5px }",
        "a { color: # }",
        "a::before { content: '' }",
        "\0\x01",
        "\r\n\t  ;;; \r\n",
        "a, b, c, .x { display: grid; grid-template-columns: repeat(, 1fr) }",
        "a { 😊: red }",
        "/* unterminated comment",
        "a { color: red /* }",
    ];

    for case in cases {
        match Stylesheet::parse(case) {
            Ok(sheet) => {
                // Reads on an accepted sheet are total functions.
                let _ = sheet.rules.len();
                let _ = sheet.font_faces();
            }
            Err(error) => {
                // Errors always carry a message and a byte offset.
                assert!(!error.message.is_empty());
            }
        }
    }
}

const ALPHABET: &[char] = &[
    'a', '{', '}', ':', ';', '(', ')', '@', '\n', '#', '.', '[', ']', '-', ',', '0', 'p', 'x',
];

#[test]
fn pseudo_random_input_never_panics() {
    let mut rng = Rng::new(0x2026_1107);
    for _ in 0..2000 {
        let length = rng.next() as usize % 300;
        let source = random_string(&mut rng, length, ALPHABET);
        if let Ok(sheet) = Stylesheet::parse(&source) {
            let _ = sheet.rules.len();
        }
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
        let _ = Stylesheet::parse(&source);
    }
}

#[test]
fn nested_calc_at_bounded_depth_terminates() {
    let depth = 4096;
    let mut declaration = "calc(".repeat(depth);
    declaration.push_str("1px");
    declaration.push_str(&")".repeat(depth));
    let source = format!("a {{ width: {} }}", declaration);
    // Either answer is fine; crashing or hanging is not.
    let _ = Stylesheet::parse(&source);
}

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
