#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        let _ = tachyon_core::fuzzing::compile_template(source, "fuzz/tac.html");
    }
});
