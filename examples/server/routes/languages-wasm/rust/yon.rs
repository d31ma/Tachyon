// In-house wasm route handler.
//
// Shaped like any Yon route handler (`struct Handler` + `impl Handler` with
// HTTP-verb methods), but written within Tachyon's supported subset so the
// framework compiles it to WebAssembly *itself* — with NO rustc and no other
// toolchain installed. Requests to `/languages-wasm/rust` execute this wasm
// in-process instead of spawning a language runtime.
//
// Subset: i32 / String / Json / bool, arithmetic, comparisons, logical
// operators, if/else, while, string concatenation, and the request helpers
// `request.len()` / `request.body()` / `request.json()`.

struct Handler;

impl Handler {
    // GET — report the byte size of the incoming request envelope.
    pub fn GET(request: Request) -> i32 {
        request.len()
    }

    // POST — a human-readable summary built with string concatenation
    // (the i32 length is coerced into the string automatically).
    pub fn POST(request: Request) -> String {
        "received " + request.len() + " bytes: " + request.body()
    }

    // PUT — triangular checksum 0 + 1 + ... + (len - 1): loops, mutable
    // locals, and assignment all run inside the wasm module.
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

    // PATCH — echo a JSON request body back as a structured JSON result.
    pub fn PATCH(request: Request) -> Json {
        json(request.body())
    }
}
