struct Handler;

impl Handler {
    pub fn GET(request: &YonJson) -> YonJson {
        match Self::status_code(request).as_str() {
            "512" => Self::response("512", "web server is returning an unknown error"),
            "513" => Self::response("513", "message too large"),
            "514" => Self::response("514", "upload failed"),
            "515" => Self::response("515", "unsupported media extension"),
            _ => Self::service().describe(request),
        }
    }

    fn service() -> RustLanguageService {
        RustLanguageService::new()
    }

    fn status_code(request: &YonJson) -> String {
        request
            .get("query")
            .and_then(|query| query.get("code"))
            .map(|value| value.as_string())
            .unwrap_or_default()
    }

    fn response(code: &str, detail: &str) -> YonJson {
        YonJson::object(vec![
            ("code", code.into()),
            ("detail", detail.into()),
        ])
    }
}
