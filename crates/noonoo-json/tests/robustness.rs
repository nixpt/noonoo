//! Robustness plus the nesting-depth limit the parser owns.
//!
//! This is the stable-CI counterpart to `fuzz/`'s nightly targets. The corpus
//! and the pseudo-random input are deterministic (fixed seed), so a failure
//! reproduces exactly.

use noonoo_json::{Json, MAX_DEPTH};

#[test]
fn adversarial_corpus_never_panics() {
    let cases = [
        "",
        "[",
        "{",
        "[}",
        "{]",
        "[1,]",
        "{'a':1}",
        "{a:1}",
        r#"{"a"}"#,
        r#"{"a":}"#,
        r#"{"a" 1}"#,
        "[1 2]",
        "\"unterminated",
        r#""\x""#,
        r#""\uD83D""#,
        "\"\u{0007}\"",
        "tru",
        "truex",
        "01",
        "1.",
        "-",
        "+1",
        "1.",
        ".5",
        "nullx",
        r#""😀""#,
        "  \r\n\t ",
        "\"a\",\"b\"",
        "0x10",
        "--1",
        "1e",
        "1e+",
        "NaN",
        r#""/""#,
        r#"" ""#,
    ];

    for case in cases {
        match Json::parse(case) {
            Ok(value) => {
                // Reads and writes on an accepted value are total functions.
                let _ = value.type_name();
                let _ = value.to_string_pretty();
            }
            Err(error) => {
                assert!(!error.message.is_empty());
            }
        }
    }
}

#[test]
fn pseudo_random_input_never_panics() {
    let mut rng = Rng::new(0x2026_1107);
    let alphabet: &[char] = &[
        '[', ']', '{', '}', ',', ':', '"', 'n', 't', 'f', '0', '1', '-', '.', ' ', '\n', 'e', '+',
    ];
    for _ in 0..2000 {
        let length = rng.next() as usize % 300;
        let source = random_string(&mut rng, length, alphabet);
        if let Ok(value) = Json::parse(&source) {
            let _ = value.to_string_pretty();
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
        let _ = Json::parse(&source);
    }
}

#[test]
fn nesting_at_and_beyond_the_limit_is_enforced() {
    // MAX_DEPTH levels of arrays still parse; a deeper pathological input is
    // rejected with the limit named, not overflowed.
    let balanced = format!(
        "{}{}",
        "[".repeat(MAX_DEPTH as usize),
        "]".repeat(MAX_DEPTH as usize)
    );
    assert!(Json::parse(&balanced).is_ok());

    let overflowing = "[".repeat(2 * MAX_DEPTH as usize + 8);
    let error = Json::parse(&overflowing).unwrap_err();
    assert!(error.message.contains("MAX_DEPTH"), "{}", error.message);
}

#[test]
fn object_nesting_hits_the_same_limit() {
    // Objects never nest deeper than MAX_DEPTH either.
    let balanced = format!(
        "{}{}",
        "{\"a\":".repeat(2 * MAX_DEPTH as usize),
        "null".to_string() + &"}".repeat(2 * MAX_DEPTH as usize),
    );
    assert!(
        Json::parse(&balanced).is_err(),
        "the depth limit applies to objects"
    );
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
