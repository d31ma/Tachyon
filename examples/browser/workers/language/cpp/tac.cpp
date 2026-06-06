// Tac Worker (C++) - a browser-local backend compiled in-house to WebAssembly.
//
// Shaped like a Yon route handler: `class Handler` with static methods named
// after HTTP verbs. Tachyon compiles it to `dist/workers/language/cpp/tac.wasm`
// with NO external toolchain (no clang/LLVM). JS/TS Tac code invokes it with
// the native fetch API - the verb selects the method (default GET):
//
//   const res = await fetch("tac://language/cpp", { method: "POST", body })
//   const { result } = (await res.json())
//
// Tac-C++ dialect (a documented subset, not full std C++): integer aliases,
// `bool`, `string`, `json`, declarations, assignment, arithmetic, comparisons, the
// ternary operator `?:` for conditionals, `while` loops, string literals +
// concatenation (i32 auto-converted), request helpers, and raw JSON responses.

class Handler {
public:
    // GET - report the size (in bytes) of the incoming request.
    static uint32_t GET(Request request) {
        return request.len();
    }

    // POST - a human-readable summary, built with ternaries and concatenation.
    static string POST(const Request& request) {
        int size = request.len();
        string tier = size > 256 ? "large" : (size > 32 ? "medium" : "small");
        return "C++ worker saw a " + tier + " request of " + size + " bytes";
    }

    // PUT - triangular checksum over the request size (0 + 1 + ... + len-1).
    static int PUT(Request request) {
        int total = 0;
        int index = 0;
        int length = request.len();
        while (index < length) {
            total = total + index;
            index = index + 1;
        }
        return total;
    }

    // PATCH - echo a JSON request body as a real object/array result.
    static json PATCH(Request request) {
        return json(request.body());
    }

    // DELETE - boolean response with logical operators.
    static bool DELETE(Request request) {
        return request.len() > 2 && true;
    }
};
