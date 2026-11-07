#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        if let Ok(value) = noonoo_json::Json::parse(source) {
            let _ = value.to_string_pretty();
        }
    }
});
