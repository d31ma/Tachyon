// Tac Worker (Go) - a browser-local compute backend compiled in-house to Wasm.
//
// Tac-Go is a small handler dialect; no Go toolchain is required.

package main

type Handler struct{}

func (Handler) GET(request Request) int32 {
    return request.len();
}

func (Handler) POST(request Request) string {
    var size = request.len();
    var tier = size > 256 ? "large" : (size > 32 ? "medium" : "small");
    return "Go worker saw a " + tier + " request of " + size + " bytes";
}

func (Handler) PUT(request Request) int {
    var total = 0;
    var index = 0;
    var length = request.len();
    for index < length {
        total = total + index;
        index = index + 1;
    }
    return total;
}

func (Handler) PATCH(request Request) json {
    return json(request.body());
}

func (Handler) DELETE(request Request) bool {
    return request.len() > 2 && true;
}
