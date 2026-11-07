#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        let doc = noonoo_html::parse(source);
        let _ = doc.elements().len();
        let _ = doc.text_content(doc.root());
    }
});
