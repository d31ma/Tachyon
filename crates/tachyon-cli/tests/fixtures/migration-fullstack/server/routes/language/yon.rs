// POST/DELETE /language — the Rust face of the consolidated polyglot route.
//
// One route, three languages: yon.js answers GET/HEAD and yon.cpp answers
// PUT/PATCH beside this file. Yon generates the dependency-free `YonJson`
// adapter, so the handler needs no third-party crates.

struct Handler;

impl Handler {
    // POST — echo the JSON request body back as a structured result.
    pub fn POST(request: &YonJson) -> YonJson {
        let body = match request.get("body") {
            Some(value) => value.clone(),
            None => YonJson::Null,
        };
        YonJson::object(vec![
            ("language", "rust".into()),
            ("action", "echo".into()),
            ("body", body),
        ])
    }

    // DELETE — confirm how many bytes the request body carried.
    pub fn DELETE(request: &YonJson) -> YonJson {
        let size = match request.get("body") {
            Some(value) => value.stringify().len(),
            None => 0,
        };
        YonJson::object(vec![
            ("language", "rust".into()),
            ("action", "size-confirm".into()),
            ("receivedBytes", size.into()),
        ])
    }
}
