#![no_main]

use libfuzzer_sys::fuzz_target;
use tachyon_contracts::NativeTarget;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        let validity = [
            NativeTarget::Linux,
            NativeTarget::Macos,
            NativeTarget::Windows,
            NativeTarget::Android,
            NativeTarget::Ios,
        ]
        .map(|target| tachyon_core::fuzzing::plan_native(target, source).is_ok());
        assert!(validity.iter().all(|value| *value == validity[0]));
    }
});
