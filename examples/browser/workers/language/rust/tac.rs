// Tac Worker - a browser-local backend compiled in-house to WebAssembly.
//
// Shaped exactly like a Yon route handler: `impl Handler` with methods named
// after HTTP request verbs. It runs in a Web Worker on the frontend, and
// Tachyon compiles it to `dist/workers/language/rust/tac.wasm` with NO external
// toolchain (no rustc). JS/TS Tac code invokes it with the native fetch API:
//
//   const res = await fetch("tac://language/rust", { method: "POST", body })
//   const { result } = (await res.json())
//
// The request verb selects the method (default GET). Each method returns an
// i32-compatible integers, bool, String, or Json, which Tachyon wraps in a JSON response:
//   { "status": 200, "body": { "method": "<VERB>", "result": <value> } }
//
// Supported subset: i32-compatible integer aliases + bool + String/Json
// params/locals, arithmetic, comparisons, logical operators, `if/else`,
// `while`, string concatenation, request helpers, and `json(...)`.

impl Handler {
    // GET - report the size (in bytes) of the incoming request.
    pub fn GET(request: Request) -> u32 {
        request.len()
    }

    // POST - a human-readable summary, built with string concatenation.
    pub fn POST(request: Request) -> String {
        let size = request.len();
        let tier = if size > 256 {
            "large"
        } else {
            if size > 32 { "medium" } else { "small" }
        };
        "processed a " + tier + " request of " + size + " bytes"
    }

    // PUT - triangular checksum over the request size: 0 + 1 + ... + (len - 1).
    // Demonstrates `while` loops, mutable locals, and assignment.
    pub fn PUT(request: Request) -> i32 {
        let mut total = 0;
        let mut index = 0;
        let length = request.len();
        while index < length {
            total = total + index;
            index = index + 1;
        }
        total
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
