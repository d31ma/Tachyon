// Tac Worker (C#) - a browser-local compute backend compiled in-house to Wasm.
//
// Tac-C# keeps the familiar `class Handler` shape but does not use Roslyn/.NET.

class Handler {
    public static uint GET(Request request) {
        return request.len();
    }

    public static string POST(Request request) {
        int size = request.len();
        string tier = size > 256 ? "large" : (size > 32 ? "medium" : "small");
        return "C# worker saw a " + tier + " request of " + size + " bytes";
    }

    public static int PUT(Request request) {
        int total = 0;
        int index = 0;
        int length = request.len();
        while (index < length) {
            total = total + index;
            index = index + 1;
        }
        return total;
    }

    public static Json PATCH(Request request) {
        return json(request.body());
    }

    public static bool DELETE(Request request) {
        return request.len() > 2 && true;
    }
}
