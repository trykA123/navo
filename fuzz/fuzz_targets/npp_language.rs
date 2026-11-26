#![no_main]

use libfuzzer_sys::fuzz_target;
use navo_syntax::from_udl;

fuzz_target!(|data: &[u8]| {
    if let Ok(xml) = std::str::from_utf8(data) {
        let _ = from_udl(xml);
    }
});
