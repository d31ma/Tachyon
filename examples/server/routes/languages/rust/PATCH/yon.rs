use crate::yon_json::JsonValue;

use crate::rust_language_service::RustLanguageService;

pub fn handler(request: &JsonValue) -> JsonValue {
    let code = request
        .get("query")
        .and_then(|query| query.get("code"))
        .and_then(|value| value.as_str().map(str::to_string).or_else(|| value.as_i64().map(|number| number.to_string())));
    if let Some(code) = code {
        if let Some(response) = status_response(&code) {
            return response;
        }
    }

    let service = RustLanguageService;
    service.patch(request)
}

fn status_response(code: &str) -> Option<JsonValue> {
    match code {
        "502" => Some(response("502", "bad gateway")),
        "503" => Some(response("503", "unavailable")),
        "504" => Some(response("504", "gateway timeout")),
        "505" => Some(response("505", "version unsupported")),
        "506" => Some(response("506", "variant negotiates")),
        "507" => Some(response("507", "insufficient storage")),
        "508" => Some(response("508", "loop detected")),
        "510" => Some(response("510", "not extended")),
        "511" => Some(response("511", "network auth required")),
        _ => None,
    }
}

fn response(code: &str, detail: &str) -> JsonValue {
    let mut body = std::collections::BTreeMap::new();
    body.insert("code".to_string(), JsonValue::String(code.to_string()));
    body.insert("detail".to_string(), JsonValue::String(detail.to_string()));
    JsonValue::Object(body)
}
