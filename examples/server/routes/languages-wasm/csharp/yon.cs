// In-house wasm route handler (C#).
//
// A `class Handler` with static HTTP-verb methods, written within Tachyon's
// supported subset so the framework compiles it to WebAssembly itself — with no
// .NET toolchain. Requests to `/languages-wasm/csharp` run this wasm in-process.

class Handler {
    // GET — report the byte size of the incoming request envelope.
    static int GET(Request request) {
        return request.len();
    }

    // POST — a human-readable summary built with string concatenation.
    static string POST(Request request) {
        return "received " + request.len() + " bytes: " + request.body();
    }

    // PATCH — echo a JSON request body back as a structured JSON result.
    static json PATCH(Request request) {
        return json(request.body());
    }
}
