namespace tachyon {

class TacMemoryEntities {
public:
    using Rows = std::map<std::string, json::JsonNode>;
    using Tables = std::map<std::string, Rows>;

    static json::JsonNode execute(const json::JsonNode& command) {
        std::string operation = command["operation"].getValue<std::string>();
        if (operation == "transaction") {
            return transaction(command);
        }
        Tables* staged = pending(command);
        std::string table = command["table"].getValue<std::string>();
        std::string idField = command["idField"].getValue<std::string>();
        if (operation == "save") {
            write(table, staged, command["value"][idField].toJsonString(), command["value"]);
            return command["value"];
        }
        if (operation == "delete") {
            write(table, staged, command["id"].toJsonString(), json::JsonNode(nullptr));
            return true;
        }
        if (operation != "query") {
            throw TacCodedError("The memory store has no operation named " + operation + ".", "storage");
        }
        const json::JsonNode& query = command["query"];
        std::vector<json::JsonNode> found;
        for (const auto& [key, row] : visible(table, staged)) {
            if (matches(row, query["where"])) {
                found.push_back(row);
            }
        }
        std::string kind = query["kind"].getValue<std::string>();
        if (kind == "count") {
            return static_cast<double>(found.size());
        }
        if (kind == "exists") {
            return !found.empty();
        }
        if (kind == "delete") {
            for (const json::JsonNode& row : found) {
                write(table, staged, row[idField].toJsonString(), json::JsonNode(nullptr));
            }
            return static_cast<double>(found.size());
        }
        return paged(found, query);
    }

    static void reset() {
        tables().clear();
        transactions().clear();
    }

private:
    static bool present(const json::JsonNode& node) {
        return node.getValueKind() != json::JsonValueKind::Null && node.getValueKind() != json::JsonValueKind::Undefined;
    }

    static Tables& tables() {
        static Tables kept;
        return kept;
    }

    static std::map<std::string, Tables>& transactions() {
        static std::map<std::string, Tables> open;
        return open;
    }

    static Tables* pending(const json::JsonNode& command) {
        if (!present(command["transaction"])) {
            return nullptr;
        }
        auto found = transactions().find(command["transaction"].getValue<std::string>());
        if (found == transactions().end()) {
            throw TacCodedError("The memory store transaction is not open.", "storage");
        }
        return &found->second;
    }

    static json::JsonNode transaction(const json::JsonNode& command) {
        static long long counter = 0;
        std::string action = command["action"].getValue<std::string>();
        if (action == "begin") {
            counter += 1;
            std::string id = "memory-" + std::to_string(counter);
            transactions()[id] = Tables();
            json::JsonObject begun;
            begun["transaction"] = id;
            return begun;
        }
        Tables staged = *pending(command);
        transactions().erase(command["transaction"].getValue<std::string>());
        if (action == "commit") {
            for (const auto& [table, rows] : staged) {
                for (const auto& [key, row] : rows) {
                    write(table, nullptr, key, row);
                }
            }
        }
        return json::JsonNode(nullptr);
    }

    static void write(const std::string& table, Tables* staged, const std::string& key, const json::JsonNode& row) {
        if (staged != nullptr) {
            (*staged)[table][key] = row;
            return;
        }
        if (row.isNull()) {
            tables()[table].erase(key);
            return;
        }
        tables()[table][key] = row;
    }

    static Rows visible(const std::string& table, const Tables* staged) {
        Rows rows = tables()[table];
        if (staged == nullptr || staged->count(table) == 0) {
            return rows;
        }
        for (const auto& [key, row] : staged->at(table)) {
            if (row.isNull()) {
                rows.erase(key);
            } else {
                rows[key] = row;
            }
        }
        return rows;
    }

    static bool matches(const json::JsonNode& row, const json::JsonNode& where) {
        for (const json::JsonNode& predicate : where.items()) {
            if (!holds(row[predicate["field"].getValue<std::string>()], predicate)) {
                return false;
            }
        }
        return true;
    }

    static bool holds(const json::JsonNode& value, const json::JsonNode& predicate) {
        std::string operation = predicate["operator"].getValue<std::string>();
        const json::JsonNode& operand = predicate["value"];
        if (operation == "isNull") {
            return !present(value);
        }
        if (operation == "isNotNull") {
            return present(value);
        }
        if (operation == "eq") {
            return value == operand;
        }
        if (operation == "ne") {
            return !(value == operand);
        }
        if (operation == "in") {
            const std::vector<json::JsonNode>& items = operand.items();
            return std::find(items.begin(), items.end(), value) != items.end();
        }
        bool comparable = value.getValueKind() == operand.getValueKind()
            && (value.getValueKind() == json::JsonValueKind::Number || value.getValueKind() == json::JsonValueKind::String);
        if (!comparable) {
            return false;
        }
        int order = compare(value, operand);
        return (operation == "lt" && order < 0) || (operation == "lte" && order <= 0)
            || (operation == "gt" && order > 0) || (operation == "gte" && order >= 0);
    }

    static int compare(const json::JsonNode& left, const json::JsonNode& right) {
        if (left.getValueKind() != right.getValueKind()) {
            return static_cast<int>(left.getValueKind()) < static_cast<int>(right.getValueKind()) ? -1 : 1;
        }
        if (left.getValueKind() == json::JsonValueKind::Number) {
            double a = left.getValue<double>();
            double b = right.getValue<double>();
            return a < b ? -1 : (a > b ? 1 : 0);
        }
        if (left.getValueKind() == json::JsonValueKind::String) {
            return left.getValue<std::string>().compare(right.getValue<std::string>());
        }
        return 0;
    }

    static json::JsonNode paged(std::vector<json::JsonNode> found, const json::JsonNode& query) {
        const json::JsonNode& order = query["orderBy"];
        std::stable_sort(found.begin(), found.end(), [&](const json::JsonNode& left, const json::JsonNode& right) {
            for (const json::JsonNode& sort : order.items()) {
                std::string field = sort["field"].getValue<std::string>();
                int compared = compare(left[field], right[field]);
                if (compared != 0) {
                    return sort["direction"].getValue<std::string>() == "desc" ? compared > 0 : compared < 0;
                }
            }
            return false;
        });
        std::size_t offset = static_cast<std::size_t>(query["page"]["offset"].getValue<double>());
        std::size_t size = static_cast<std::size_t>(query["page"]["size"].getValue<double>());
        json::JsonArray page;
        for (std::size_t index = offset; index < found.size() && index < offset + size; index += 1) {
            page.add(found[index]);
        }
        return page;
    }
};

}
