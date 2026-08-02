//! Internal surfaces exposed to fuzz targets.
//!
//! This module exists only under the `fuzzing` feature. It is not a stable
//! API, carries no compatibility guarantee, and must never be enabled by a
//! released artifact. It exists so `fuzz/` can reach the parsers and decoders
//! that sit on a trust boundary but are otherwise crate-private.

use crate::Failure;
use crate::template::TemplateFrontend;
use std::collections::BTreeSet;
use tachyon_contracts::{HandlerResponse, NativeTarget, NativeUi};

/// Compiles one template source exactly as the web compiler does.
///
/// # Errors
///
/// Returns the same diagnostics the compiler would return.
pub fn compile_template(source: &str, source_path: &str) -> Result<(), Failure> {
    TemplateFrontend::compile(source, source_path, &BTreeSet::new()).map(|_| ())
}

/// Decodes one length-prefixed Handler Protocol v1 response frame.
///
/// This is the decoder that reads untrusted child-process output.
///
/// # Errors
///
/// Returns a protocol diagnostic for every malformed or oversized frame.
pub fn decode_response_frame(bytes: &[u8], request_id: &str) -> Result<HandlerResponse, Failure> {
    crate::handler::frame::response_frame(bytes, request_id)
}

/// Plans one resolved document into Native UI v1 for the given target.
///
/// # Errors
///
/// Returns the same planning diagnostics a native build would return.
pub fn plan_native(target: NativeTarget, html: &str) -> Result<NativeUi, Failure> {
    crate::native::plan_for_fuzzing(target, html)
}
