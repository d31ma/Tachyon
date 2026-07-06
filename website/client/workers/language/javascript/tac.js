// Tac Worker (JavaScript) - familiar JS syntax compiled in-house to Wasm.
//
// This is a documented Tachyon subset, not arbitrary JavaScript execution.
// Use JSDoc return annotations when the compiler cannot infer the result type.

class Handler {
    /** @param {TacWorkerRequest} request */
    GET(request) {
        return request.len();
    }

    /**
     * @param {TacWorkerRequest} request
     * @returns {string}
     */
    POST(request) {
        const size = request.len();
        const tier = size > 256 ? "large" : (size > 32 ? "medium" : "small");
        return "JavaScript worker saw a " + tier + " request of " + size + " bytes";
    }

    /** @param {TacWorkerRequest} request */
    PUT(request) {
        let total = 0;
        let index = 0;
        const length = request.len();
        while (index < length) {
            total = total + index;
            index = index + 1;
        }
        return total;
    }

    /**
     * @param {TacWorkerRequest} request
     * @returns {Json}
     */
    PATCH(request) {
        return json(request.body());
    }

    /**
     * @param {TacWorkerRequest} request
     * @returns {boolean}
     */
    DELETE(request) {
        return request.len() > 2 && true;
    }
}
