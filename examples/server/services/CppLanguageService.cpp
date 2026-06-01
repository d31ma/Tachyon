#include <string>
#include "YonJson.hpp"
#include "CppFyloRepository.cpp"

class CppLanguageService {
public:
    YonJson describe(const YonJson& request) const {
        std::string requestId = "unknown";
        if (const YonJson* context = request.get("context")) {
            if (const YonJson* value = context->get("requestId")) {
                requestId = value->asString("unknown");
            }
        }

        return YonJson::object({
            {"language", "cpp"},
            {"message", "Hello from C++!"},
            {"requestId", requestId},
            {"fylo", repository_.inspectAndRebuild()},
        });
    }

private:
    CppFyloRepository repository_;
};
