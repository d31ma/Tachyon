namespace tachyon {

class TacHost {
public:
    static std::string storage(const std::string& text) {
        json::JsonNode request = json::JsonNode::parse(text);
        std::string operation = request["op"].getValueKind() == json::JsonValueKind::String ? request["op"].getValue<std::string>() : std::string();
        if (operation == "entity") {
            return entity(request["command"]);
        }
        if (operation.rfind("cache.", 0) != 0) {
            return missing();
        }
        std::string key = request["cache"].toJsonString() + request["key"].toJsonString();
        json::JsonObject answer;
        if (operation == "cache.get") {
            json::JsonObject entry;
            auto found = cached().find(key);
            entry["hit"] = found != cached().end();
            if (found != cached().end()) {
                entry["value"] = found->second["value"];
                entry["expiresAt"] = found->second["expiresAt"];
            }
            answer["value"] = entry;
        } else if (operation == "cache.set") {
            cached()[key] = request;
            answer["value"] = nullptr;
        } else {
            cached().erase(key);
            answer["value"] = nullptr;
        }
        return answer.toJsonString();
    }

    static void waitMilliseconds(double) {}

    static void reset() {
        cached().clear();
        TacMemoryEntities::reset();
    }

    static std::string web(const std::string&) {
        return missing();
    }

    static double epochMilliseconds() {
        return 0;
    }

    static void httpStart(std::uint32_t, const std::string&) {}

    static void httpCancel(std::uint32_t) {}

private:
    static std::string entity(const json::JsonNode& command) {
        json::JsonObject answer;
        try {
            answer["value"] = TacMemoryEntities::execute(command);
        } catch (const std::exception& problem) {
            answer["error"] = problem.what();
        }
        return answer.toJsonString();
    }

    static std::map<std::string, json::JsonNode>& cached() {
        static std::map<std::string, json::JsonNode> entries;
        return entries;
    }

    static std::string missing() {
        json::JsonObject failure;
        failure["error"] = "Browser APIs are not in C++ test units yet (ADR 0099).";
        return failure.toJsonString();
    }
};

}

namespace tachyon {

class TacTaskOwner {
public:
    static std::string current() {
        return std::string();
    }

    static void resumeFor(const std::string&, std::coroutine_handle<> waiting) {
        waiting.resume();
    }

    static void finish(std::uint32_t, const std::string&, const std::string&) {}
};

}
