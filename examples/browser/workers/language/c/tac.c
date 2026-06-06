// Tac Worker (C) - a browser-local compute backend compiled in-house to Wasm.
//
// Tac-C uses top-level HTTP verb functions. Tachyon wraps them in the shared
// worker ABI and emits dist/workers/language/c/tac.wasm without clang/LLVM.

unsigned int GET(Request request) {
    return request.len();
}

string POST(Request request) {
    int size = request.len();
    string tier = size > 256 ? "large" : (size > 32 ? "medium" : "small");
    return "C worker saw a " + tier + " request of " + size + " bytes";
}

int PUT(Request request) {
    int total = 0;
    int index = 0;
    int length = request.len();
    while (index < length) {
        total = total + index;
        index = index + 1;
    }
    return total;
}

json PATCH(Request request) {
    return json(request.body());
}

bool DELETE(Request request) {
    return request.len() > 2 && true;
}
