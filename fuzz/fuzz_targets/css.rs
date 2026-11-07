#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        if let Ok(sheet) = noonoo_css::Stylesheet::parse(source) {
            let _ = sheet.rules.len();
            let _ = sheet.font_faces().len();
        }
    }
});
