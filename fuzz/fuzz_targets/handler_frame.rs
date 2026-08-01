#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = tachyon_core::fuzzing::decode_response_frame(data, "fuzz-request");
});
