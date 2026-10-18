namespace tachyon {

class TacBehaviorData {
public:
    static TacCodedError invalid() {
        return TacCodedError("Invalid or out-of-bounds behavior data.", "invalid");
    }

    static bool present(const json::JsonNode& node) {
        return node.getValueKind() != json::JsonValueKind::Null && node.getValueKind() != json::JsonValueKind::Undefined;
    }

    static long long integer(const json::JsonNode& node, long long fallback, long long minimum, long long maximum) {
        if (!present(node)) {
            return fallback;
        }
        if (node.getValueKind() != json::JsonValueKind::Number) {
            throw invalid();
        }
        double number = node.getValue<double>();
        bool whole = std::isfinite(number) && number == std::trunc(number);
        if (!whole || number < static_cast<double>(minimum) || number > static_cast<double>(maximum)) {
            throw invalid();
        }
        return static_cast<long long>(number);
    }

    static std::string bounded(const json::JsonNode& node, std::size_t limit) {
        std::string text = node.toJsonString();
        if (text.size() > limit) {
            throw invalid();
        }
        return text;
    }

    static json::JsonNode ask(const json::JsonNode& request) {
        json::JsonNode answer;
        try {
            answer = json::JsonNode::parse(TacHost::storage(request.toJsonString()));
        } catch (const json::JsonException&) {
            throw TacCodedError("Tachyon storage did not answer.", "storage");
        }
        if (answer.getValueKind() != json::JsonValueKind::Object) {
            throw TacCodedError("Tachyon storage did not answer.", "storage");
        }
        if (answer["error"].getValueKind() == json::JsonValueKind::String) {
            throw TacCodedError(answer["error"].getValue<std::string>(), "storage");
        }
        return answer["value"];
    }

    static bool listed(const json::JsonNode& list, const std::string& text) {
        for (const json::JsonNode& item : list.items()) {
            if (item.getValueKind() == json::JsonValueKind::String && item.getValue<std::string>() == text) {
                return true;
            }
        }
        return false;
    }
};

class TacPageRequest {
public:
    static json::JsonNode normalized(const json::JsonNode& input, const json::JsonNode& policy) {
        TacBehaviorData::bounded(input, 4096);
        if (input.getValueKind() != json::JsonValueKind::Object) {
            throw TacBehaviorData::invalid();
        }
        for (const std::string& key : input.propertyNames()) {
            if (key != "page" && key != "size" && key != "sort" && key != "offset") {
                throw TacBehaviorData::invalid();
            }
        }
        long long defaultSize = TacBehaviorData::integer(policy["defaultSize"], 20, 1, 1000);
        long long maxSize = TacBehaviorData::integer(policy["maxSize"], 100, 1, 1000);
        long long page = TacBehaviorData::integer(input["page"], 0, 0, 1000000);
        long long size = TacBehaviorData::integer(input["size"], defaultSize, 1, maxSize);
        long long offset = page * size;
        if (offset > 1000000 || TacBehaviorData::integer(input["offset"], offset, 0, 1000000) != offset) {
            throw TacBehaviorData::invalid();
        }
        json::JsonObject normalized;
        normalized["page"] = static_cast<double>(page);
        normalized["size"] = static_cast<double>(size);
        normalized["offset"] = static_cast<double>(offset);
        normalized["sort"] = sorted(input["sort"], policy["sort"]);
        return normalized;
    }

private:
    static json::JsonNode sorted(const json::JsonNode& written, const json::JsonNode& allowed) {
        json::JsonArray sort;
        if (!TacBehaviorData::present(written)) {
            return sort;
        }
        if (written.getValueKind() != json::JsonValueKind::Array || written.count() > 16) {
            throw TacBehaviorData::invalid();
        }
        std::vector<std::string> seen;
        for (const json::JsonNode& entry : written.items()) {
            if (entry.getValueKind() != json::JsonValueKind::String) {
                throw TacBehaviorData::invalid();
            }
            std::string text = entry.getValue<std::string>();
            std::size_t colon = text.find(':');
            std::string field = text.substr(0, colon);
            std::string direction = colon == std::string::npos ? "asc" : text.substr(colon + 1);
            bool repeated = std::find(seen.begin(), seen.end(), field) != seen.end();
            bool known = TacBehaviorData::listed(allowed, field);
            bool directed = direction == "asc" || direction == "desc";
            if (repeated || !known || !directed) {
                throw TacBehaviorData::invalid();
            }
            seen.push_back(field);
            sort.add(field + ":" + direction);
        }
        return sort;
    }
};

class TacTransactions {
public:
    template <typename Work>
    static auto within(const json::JsonNode& policy, Work work) -> decltype(work()) {
        if (!TacBehaviorData::present(policy)) {
            return work();
        }
        std::string adapter = policy["adapter"].getValue<std::string>();
        if (!open(adapter).empty()) {
            return work();
        }
        if (adapter != "indexeddb") {
            throw TacCodedError("This entity adapter has no transactions.", "adapter");
        }
        json::JsonNode begun = TacBehaviorData::ask(control("begin", ""));
        std::string id = begun.getValueKind() == json::JsonValueKind::Object && begun["transaction"].getValueKind() == json::JsonValueKind::String
            ? begun["transaction"].getValue<std::string>()
            : std::string();
        if (id.empty() || id.size() > 128) {
            throw TacBehaviorData::invalid();
        }
        TacOpen opened(adapter, id);
        try {
            if constexpr (std::is_void_v<decltype(work())>) {
                work();
                opened.close();
                TacBehaviorData::ask(control("commit", id));
            } else {
                auto value = work();
                opened.close();
                TacBehaviorData::ask(control("commit", id));
                return value;
            }
        } catch (...) {
            opened.close();
            try {
                TacBehaviorData::ask(control("rollback", id));
            } catch (const std::exception&) {
            }
            throw;
        }
    }

    static std::string& open(const std::string& adapter) {
        static std::map<std::string, std::string> transactions;
        return transactions[adapter];
    }

private:
    class TacOpen {
    public:
        TacOpen(std::string adapter, const std::string& id) : adapter(std::move(adapter)) {
            open(this->adapter) = id;
        }

        ~TacOpen() {
            close();
        }

        void close() {
            open(adapter).clear();
        }

        TacOpen(const TacOpen&) = delete;
        TacOpen& operator=(const TacOpen&) = delete;

    private:
        std::string adapter;
    };

    static json::JsonNode control(const char* action, const std::string& id) {
        json::JsonObject command;
        command["operation"] = "transaction";
        command["action"] = action;
        if (!id.empty()) {
            command["transaction"] = id;
        }
        json::JsonObject request;
        request["op"] = "entity";
        request["command"] = command;
        return request;
    }
};

class TacBehaviors {
public:
    template <typename Result, typename Encode, typename Body>
    static Result run(const json::JsonNode& policy, Encode encode, Body body) {
        std::vector<json::JsonNode> arguments = encode();
        if (arguments.size() > 64) {
            throw TacBehaviorData::invalid();
        }
        for (json::JsonNode& argument : arguments) {
            TacBehaviorData::bounded(argument, 1048576);
        }
        const json::JsonNode& page = policy["pageable"];
        if (TacBehaviorData::present(page)) {
            std::size_t index = static_cast<std::size_t>(TacBehaviorData::integer(page["argument"], 0, 0, 31));
            if (index >= arguments.size()) {
                throw TacBehaviorData::invalid();
            }
            arguments[index] = TacPageRequest::normalized(arguments[index], page);
        }
        if (TacBehaviorData::present(policy["mutation"])) {
            return mutate<Result>(policy, arguments, body);
        }
        if constexpr (!std::is_void_v<Result>) {
            if (TacBehaviorData::present(policy["keyArgs"])) {
                return cached<Result>(policy, arguments, body);
            }
        }
        return result<Result>(policy, arguments, body);
    }

    static void reset() {
        slots().clear();
    }

private:
    static std::map<std::string, long long>& slots() {
        static std::map<std::string, long long> taken;
        return taken;
    }

    class TacSlot {
    public:
        TacSlot(std::string key, long long limit) : key(std::move(key)) {
            long long& taken = slots()[this->key];
            if (taken >= limit) {
                throw TacCodedError("Behavior method concurrency limit reached.", "capacity");
            }
            taken += 1;
        }

        ~TacSlot() {
            long long& taken = slots()[key];
            taken -= 1;
            if (taken <= 0) {
                slots().erase(key);
            }
        }

        TacSlot(const TacSlot&) = delete;
        TacSlot& operator=(const TacSlot&) = delete;

    private:
        std::string key;
    };

    template <typename Result, typename Body>
    static Result cached(const json::JsonNode& policy, const std::vector<json::JsonNode>& arguments, Body& body) {
        const json::JsonNode& cache = policy["cache"];
        std::string key = cacheKey(policy, policy["name"], policy["keyArgs"], arguments);
        json::JsonNode entry = TacBehaviorData::ask(request("cache.get", cache, key));
        if (fresh(entry, cache)) {
            return json::JsonCodec<Result>::fromJson(entry["value"]);
        }
        Result value = result<Result>(policy, arguments, body);
        write(cache, key, json::JsonCodec<Result>::toJson(value));
        return value;
    }

    template <typename Result, typename Body>
    static Result mutate(const json::JsonNode& policy, const std::vector<json::JsonNode>& arguments, Body& body) {
        const json::JsonNode& mutation = policy["mutation"];
        const json::JsonNode& cache = policy["cache"];
        std::vector<json::JsonNode> mapped;
        for (const json::JsonNode& index : mutation["arguments"].items()) {
            std::size_t at = static_cast<std::size_t>(TacBehaviorData::integer(index, 0, 0, 31));
            if (at >= arguments.size()) {
                throw TacBehaviorData::invalid();
            }
            mapped.push_back(arguments[at]);
        }
        const json::JsonNode& page = mutation["pageable"];
        if (TacBehaviorData::present(page)) {
            std::size_t index = static_cast<std::size_t>(TacBehaviorData::integer(page["argument"], 0, 0, 31));
            if (index >= mapped.size()) {
                throw TacBehaviorData::invalid();
            }
            mapped[index] = TacPageRequest::normalized(mapped[index], page);
        }
        std::string key = cacheKey(policy, mutation["method"], mutation["keyArgs"], mapped);
        bool putting = mutation["operation"].getValue<std::string>() == "cache:put";
        if constexpr (std::is_void_v<Result>) {
            result<Result>(policy, arguments, body);
            TacBehaviorData::ask(request("cache.delete", cache, key));
        } else {
            Result value = result<Result>(policy, arguments, body);
            if (putting) {
                write(cache, key, json::JsonCodec<Result>::toJson(value));
            } else {
                TacBehaviorData::ask(request("cache.delete", cache, key));
            }
            return value;
        }
    }

    template <typename Result, typename Body>
    static Result result(const json::JsonNode& policy, const std::vector<json::JsonNode>& arguments, Body& body) {
        const json::JsonNode& limit = policy["limit"];
        if (!TacBehaviorData::present(limit)) {
            return attempt<Result>(policy, arguments, body);
        }
        std::string key = policy["owner"].getValue<std::string>() + "." + policy["name"].getValue<std::string>();
        TacSlot slot(key, TacBehaviorData::integer(limit["limit"], 1, 1, 64));
        return attempt<Result>(policy, arguments, body);
    }

    template <typename Result, typename Body>
    static Result attempt(const json::JsonNode& policy, const std::vector<json::JsonNode>& arguments, Body& body) {
        const json::JsonNode& retry = policy["retry"];
        if (!TacBehaviorData::present(retry)) {
            return transacted<Result>(policy, arguments, body);
        }
        long long retries = TacBehaviorData::integer(retry["maxRetries"], 1, 1, 8);
        for (long long attempt = 0;; attempt += 1) {
            try {
                return transacted<Result>(policy, arguments, body);
            } catch (const TacCodedError& problem) {
                if (attempt == retries || !retryable(problem.code())) {
                    throw;
                }
            } catch (const std::exception&) {
                if (attempt == retries) {
                    throw;
                }
            }
            TacHost::waitMilliseconds(backoff(retry, attempt));
        }
    }

    template <typename Result, typename Body>
    static Result transacted(const json::JsonNode& policy, const std::vector<json::JsonNode>& arguments, Body& body) {
        return TacTransactions::within(policy["transactional"], [&]() -> Result { return body(arguments); });
    }

    static bool retryable(const std::string& code) {
        static const std::vector<std::string> final = {"invalid", "unauthorized", "capacity", "cancelled", "CANCELLED", "timeout", "configuration"};
        return std::find(final.begin(), final.end(), code) == final.end();
    }

    static double backoff(const json::JsonNode& retry, long long attempt) {
        double delay = static_cast<double>(TacBehaviorData::integer(retry["delayMs"], 0, 0, 60000));
        double multiplier = TacBehaviorData::present(retry["multiplier"]) ? retry["multiplier"].getValue<double>() : 1.0;
        double ceiling = static_cast<double>(TacBehaviorData::integer(retry["maxDelayMs"], 60000, 0, 600000));
        long long jitter = TacBehaviorData::integer(retry["jitterMs"], 0, 0, 60000);
        double waited = std::min(delay * std::pow(multiplier, static_cast<double>(attempt)), ceiling);
        if (jitter > 0) {
            waited += static_cast<double>(static_cast<long long>(TacHost::epochMilliseconds()) % (jitter + 1));
        }
        return std::max(std::round(waited), 0.0);
    }

    static std::string cacheKey(const json::JsonNode& policy, const json::JsonNode& method, const json::JsonNode& keys, const std::vector<json::JsonNode>& arguments) {
        json::JsonArray selected;
        std::vector<long long> seen;
        for (const json::JsonNode& index : keys.items()) {
            long long at = TacBehaviorData::integer(index, 0, 0, 31);
            if (std::find(seen.begin(), seen.end(), at) != seen.end() || static_cast<std::size_t>(at) >= arguments.size()) {
                throw TacBehaviorData::invalid();
            }
            seen.push_back(at);
            TacBehaviorData::bounded(arguments[static_cast<std::size_t>(at)], 4096);
            selected.add(arguments[static_cast<std::size_t>(at)]);
        }
        json::JsonArray key;
        key.add(policy["partition"]);
        key.add(policy["cache"]["name"]);
        key.add(policy["module"]);
        key.add(policy["owner"]);
        key.add(method);
        key.add(selected);
        return TacBehaviorData::bounded(key, 4096);
    }

    static json::JsonNode request(const char* operation, const json::JsonNode& cache, const std::string& key) {
        json::JsonObject request;
        request["op"] = operation;
        request["cache"] = cache["name"];
        request["key"] = key;
        return request;
    }

    static bool fresh(const json::JsonNode& entry, const json::JsonNode& cache) {
        if (entry.getValueKind() != json::JsonValueKind::Object || entry["hit"].getValueKind() != json::JsonValueKind::True) {
            return false;
        }
        double now = TacHost::epochMilliseconds();
        double expiresAt = entry["expiresAt"].getValueKind() == json::JsonValueKind::Number ? entry["expiresAt"].getValue<double>() : 0;
        if (expiresAt > now + cache["ttlMs"].getValue<double>()) {
            throw TacBehaviorData::invalid();
        }
        if (expiresAt <= now) {
            return false;
        }
        TacBehaviorData::bounded(entry["value"], static_cast<std::size_t>(cache["maxBytes"].getValue<double>()));
        return true;
    }

    static void write(const json::JsonNode& cache, const std::string& key, const json::JsonNode& value) {
        TacBehaviorData::bounded(value, static_cast<std::size_t>(cache["maxBytes"].getValue<double>()));
        json::JsonNode sent = request("cache.set", cache, key);
        sent["value"] = value;
        sent["expiresAt"] = TacHost::epochMilliseconds() + cache["ttlMs"].getValue<double>();
        sent["maxEntries"] = cache["maxEntries"];
        TacBehaviorData::ask(sent);
    }


};

}
