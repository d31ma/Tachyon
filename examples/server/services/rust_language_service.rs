use crate::yon_json::JsonValue;
use std::collections::BTreeMap;

pub struct RustLanguageService;

impl RustLanguageService {
    pub fn patch(&self, request: &JsonValue) -> JsonValue {
        let request_id = request
            .get("context")
            .and_then(|context| context.get("requestId"))
            .and_then(JsonValue::as_str)
            .unwrap_or("unknown");

        let mut response = BTreeMap::new();
        response.insert("message".to_string(), JsonValue::String("Hello from Rust!".to_string()));
        response.insert("requestId".to_string(), JsonValue::String(request_id.to_string()));
        JsonValue::Object(response)
    }
}
