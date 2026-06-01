#include <cstdio>
#include <cstdlib>
#include <sstream>
#include <stdexcept>
#include <string>
#include "YonJson.hpp"

class CppFyloRepository {
public:
    YonJson inspectAndRebuild() const {
        const std::string collection = "language-route-events";
        machine(YonJson::object({{"op", "createCollection"}, {"collection", collection}}));
        machine(YonJson::object({{"op", "inspectCollection"}, {"collection", collection}}));
        machine(YonJson::object({{"op", "rebuildCollection"}, {"collection", collection}}));
        return YonJson::object({
            {"collection", collection},
            {"operations", YonJson::array({"createCollection", "inspectCollection", "rebuildCollection"})},
            {"resultCount", "3"},
        });
    }

private:
    YonJson machine(const YonJson& request) const {
        const std::string root = envOr("FYLO_ROOT", "db");
        const char* executable = std::getenv("FYLO_EXEC_PATH");
        const std::string command = executable && std::string(executable).size() > 0
            ? shellQuote(executable) + " exec --request - --root " + shellQuote(root)
            : "bunx --bun fylo.exec exec --request - --root " + shellQuote(root);
        FILE* pipe = popen(("printf %s " + shellQuote(request.stringify()) + " | " + command).c_str(), "r");
        if (pipe == nullptr) throw std::runtime_error("Unable to start fylo.exec");
        std::ostringstream out;
        char buffer[512];
        while (fgets(buffer, sizeof(buffer), pipe) != nullptr) out << buffer;
        const int status = pclose(pipe);
        if (status != 0) throw std::runtime_error("fylo.exec failed");
        YonJson response = YonJson::parse(out.str());
        const YonJson* ok = response.get("ok");
        if (ok == nullptr || ok->asString() != "true") throw std::runtime_error("fylo.exec returned an error");
        const YonJson* result = response.get("result");
        return result == nullptr ? YonJson() : *result;
    }

    static std::string envOr(const char* name, const std::string& fallback) {
        const char* value = std::getenv(name);
        return value == nullptr || std::string(value).empty() ? fallback : std::string(value);
    }

    static std::string shellQuote(const std::string& value) {
        std::string out = "'";
        for (char ch : value) out += ch == '\'' ? "'\\''" : std::string(1, ch);
        out += "'";
        return out;
    }
};
