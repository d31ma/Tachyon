// PUT/PATCH /language — the C++ face of the consolidated polyglot route.
//
// One route, three languages: yon.js answers GET/HEAD and yon.rs answers
// POST/DELETE beside this file. Yon generates the dependency-free `YonJson`
// helper, so the handler compiles with the system toolchain alone.

class Handler {
public:
    // PUT — echo the JSON request body back as a structured result.
    static YonJson PUT(const YonJson& request) {
        const YonJson* body = request.get("body");
        return YonJson::object({
            {"language", "cpp"},
            {"action", "echo"},
            {"body", body == nullptr ? YonJson() : *body},
        });
    }

    // PATCH — summarize the size of the incoming patch envelope.
    static YonJson PATCH(const YonJson& request) {
        const YonJson* body = request.get("body");
        const std::string payload = body == nullptr ? "" : body->stringify();
        return YonJson::object({
            {"language", "cpp"},
            {"action", "patch-summary"},
            {"receivedBytes", static_cast<double>(payload.size())},
        });
    }
};
