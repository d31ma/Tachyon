// Tac Worker (Zig) - a browser-local compute backend compiled in-house to Wasm.
//
// Tac-Zig is a documented subset shaped like `const Handler = struct { ... }`.
// Tachyon parses the subset directly; no zig compiler is required.

const Handler = struct {
    pub fn GET(request: Request) u32 {
        return request.len();
    }

    pub fn POST(request: Request) string {
        const size = request.len();
        const tier = size > 256 ? "large" : (size > 32 ? "medium" : "small");
        return "Zig worker saw a " + tier + " request of " + size + " bytes";
    }

    pub fn PUT(request: Request) i32 {
        var total = 0;
        var index = 0;
        const length = request.len();
        while (index < length) {
            total = total + index;
            index = index + 1;
        }
        return total;
    }

    pub fn PATCH(request: Request) json {
        return json(request.body());
    }

    pub fn DELETE(request: Request) bool {
        return request.len() > 2 && true;
    }
};
