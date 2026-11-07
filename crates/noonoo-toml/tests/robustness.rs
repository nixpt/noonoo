//! Robustness plus the nesting-depth limit the parser owns.
//!
//! This is the stable-CI counterpart to `fuzz/`'s nightly targets. The corpus
//! and the pseudo-random input are deterministic (fixed seed), so a failure
//! reproduces exactly.

use noonoo_toml::{MAX_DEPTH, to_json};

#[test]
fn adversarial_corpus_never_panics() {
    let cases = [
        "",
        "[",
        "]",
        "[table",
        "[.]",
        "[table x]",
        "[table]x = 1",
        "x =",
        "x = ",
        "x = \"unterminated",
        "x = 'unterminated",
        "x = [1,",
        "x = {a",
        "x = {a=,}",
        "x = {a=1",
        "x = {a=}{b=2}",
        "x = y = 1",
        "# only a comment",
        "\0\x01",
        "\r\n  \t[crlf]\r\nx = 1\r\n",
        "[table]\nx = 1\nx = 2",
        "[table]\n[t64able.sub]\nx = 'ü日'",
        "truex = 1",
        "x = 1979-05-27",
        "x = inf",
        "x = nan",
        "",
    ];

    for case in cases {
        match to_json(case) {
            Ok(value) => {
                let _ = value.to_string();
            }
            Err(error) => {
                assert!(!error.message.is_empty());
            }
        }
    }
}

const ALPHABET: &[char] = &[
    '[', ']', '{', '}', '=', '"', '\'', ',', '\n', '.', '-', 'a', '1',
];

#[test]
fn pseudo_random_input_never_panics() {
    let mut rng = Rng::new(0x2026_1107);
    for _ in 0..2000 {
        let length = rng.next() as usize % 300;
        let source = random_string(&mut rng, length, ALPHABET);
        if let Ok(value) = to_json(&source) {
            let _ = value.to_string();
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
        let _ = to_json(&source);
    }
}

#[test]
fn inline_nesting_is_enforced_not_overflowed() {
    let open = format!("x = {}", "{a=".repeat(2 * MAX_DEPTH as usize));
    let error = to_json(&open).unwrap_err();
    assert!(error.message.contains("MAX_DEPTH"), "{}", error.message);
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
