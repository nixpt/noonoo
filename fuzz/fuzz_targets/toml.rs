#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        if let Ok(value) = noonoo_toml::to_json(source) {
            let _ = value.to_string();
        }
    }
});
