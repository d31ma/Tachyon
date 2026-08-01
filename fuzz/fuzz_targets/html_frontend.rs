#![no_main]

use libfuzzer_sys::fuzz_target;
use tachyon_core::HtmlFrontend;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        let _ = HtmlFrontend::parse(source, "fuzz/tac.html");
    }
});
