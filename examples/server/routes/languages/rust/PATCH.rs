use crate::yon_json::JsonValue;

use crate::rust_language_service::RustLanguageService;

pub fn handler(request: &JsonValue) -> JsonValue {
    let service = RustLanguageService;
    service.patch(request)
}
