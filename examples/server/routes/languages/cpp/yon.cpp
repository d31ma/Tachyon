#include "CppLanguageService.cpp"

class Handler {
public:
    static YonJson GET(const YonJson& request) {
        const std::string code = statusCode(request);
        if (code == "502") return response("502", "bad gateway");
        if (code == "503") return response("503", "service unavailable");
        if (code == "504") return response("504", "gateway timeout");
        if (code == "505") return response("505", "http version not supported");
        if (code == "506") return response("506", "variant also negotiates");

        return service().describe(request);
    }

private:
    static CppLanguageService& service() {
        static CppLanguageService instance;
        return instance;
    }

    static std::string statusCode(const YonJson& request) {
        const YonJson* query = request.get("query");
        if (query == nullptr) return "";
        const YonJson* code = query->get("code");
        return code == nullptr ? "" : code->asString();
    }

    static YonJson response(const std::string& code, const std::string& detail) {
        return YonJson::object({
            {"code", code},
            {"detail", detail},
        });
    }
};
