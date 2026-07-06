// Tac Worker (TypeScript) - typed JS-family syntax compiled in-house to Wasm.
//
// Type annotations describe the Wasm ABI result shape; Tachyon does not run tsc.

export default class Handler {
    GET(request: TacWorkerRequest): number {
        return request.len();
    }

    POST(request: TacWorkerRequest): string {
        const size: number = request.len();
        const tier: string = size > 256 ? "large" : (size > 32 ? "medium" : "small");
        return "TypeScript worker saw a " + tier + " request of " + size + " bytes";
    }

    PUT(request: TacWorkerRequest): number {
        let total: number = 0;
        let index: number = 0;
        const length: number = request.len();
        while (index < length) {
            total = total + index;
            index = index + 1;
        }
        return total;
    }

    PATCH(request: TacWorkerRequest): Json {
        return json(request.body());
    }

    DELETE(request: TacWorkerRequest): boolean {
        return request.len() > 2 && true;
    }
}
