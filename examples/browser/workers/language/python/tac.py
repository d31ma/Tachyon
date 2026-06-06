# Tac Worker (Python) - a browser-local backend compiled in-house to WebAssembly.
#
# Shaped like a Yon route handler: a `class Handler` with static methods named
# after HTTP verbs. Tachyon compiles it to `dist/workers/language/python/tac.wasm`
# with NO external toolchain (no CPython/Pyodide/Emscripten). Invoke from JS/TS
# with the fetch API:
#
#   const res = await fetch("tac://language/python", { method: "POST", body })
#
# Tac-Python dialect: int/bool/str/json return types, assignment, Python ternary
# expressions, logical operators, while loops, string concatenation,
# request.len() / request.body() / request.json(), and json(string_expr).

class Handler:
    # GET - report the size (in bytes) of the incoming request.
    @staticmethod
    def GET(request) -> int:
        return request.len()

    # POST - a human-readable summary built with Python ternaries.
    @staticmethod
    def POST(request) -> str:
        size = request.len()
        tier = "large" if size > 256 else ("medium" if size > 32 else "small")
        return "Python worker saw a " + tier + " request of " + size + " bytes"

    # PUT - triangular checksum over the request size (0 + 1 + ... + len-1).
    @staticmethod
    def PUT(request) -> int:
        total = 0
        index = 0
        length = request.len()
        while index < length:
            total = total + index
            index = index + 1
        return total

    # PATCH - echo a JSON request body as a real object/array result.
    @staticmethod
    def PATCH(request) -> json:
        return json(request.body())

    # DELETE - boolean response with logical operators.
    @staticmethod
    def DELETE(request) -> bool:
        return request.len() > 2 and True
