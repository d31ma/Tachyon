pub struct RustLanguageService;

impl RustLanguageService {
    pub fn new() -> RustLanguageService {
        RustLanguageService
    }

    pub fn describe(&self, request: &YonJson) -> YonJson {
        let request_id = request
            .get("context")
            .and_then(|context| context.get("requestId"))
            .map(|value| value.as_string())
            .unwrap_or_else(|| "unknown".to_string());

        YonJson::object(vec![
            ("language", "rust".into()),
            ("message", "Hello from Rust!".into()),
            ("requestId", request_id.into()),
            ("fylo", RustFyloRepository::new().disposable_sample()),
        ])
    }
}
