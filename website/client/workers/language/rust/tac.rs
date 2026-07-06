// Tac Worker - a browser-local backend compiled in-house to WebAssembly.
//
// Shaped exactly like a Yon route handler: `impl Handler` with methods named
// after HTTP request verbs. On the web target Tachyon compiles it to
// `dist/web/workers/language/rust/rs.wasm` with NO external toolchain; on
// desktop targets it compiles to a native executable with the system rustc.
// JS/TS Tac code invokes it with the native fetch API:
//
//   const res = await fetch("tac://language/rust", { method: "POST", body })
//   const { result } = (await res.json())
//
// The request verb selects the method (default GET). Each method returns an
// i32, bool, String, or Json, which Tachyon wraps in a JSON response:
//   { "status": 200, "body": { "method": "<VERB>", "result": <value> } }
//
// Written in the intersection of the in-house wasm subset and plain Rust so
// the same source builds for web (wasm) and desktop (native) targets.

impl Handler {
    // GET - report the size (in bytes) of the incoming request.
    pub fn GET(request: Request) -> i32 {
        request.len()
    }

    // POST - triangular checksum over the request size: 0 + 1 + ... + (len - 1).
    // Demonstrates `while` loops, mutable locals, and assignment.
    pub fn POST(request: Request) -> i32 {
        let mut total = 0;
        let mut index = 0;
        let length = request.len();
        while index < length {
            total = total + index;
            index = index + 1;
        }
        total
    }

    // PUT - arithmetic on the request size (bytes -> bits).
    pub fn PUT(request: Request) -> i32 {
        request.len() * 8
    }

    // PATCH - echo a JSON request body as a real object/array result.
    pub fn PATCH(request: Request) -> Json {
        json(request.body())
    }

    // DELETE - boolean response with logical operators.
    pub fn DELETE(request: Request) -> bool {
        request.len() > 2 && true
    }
}
