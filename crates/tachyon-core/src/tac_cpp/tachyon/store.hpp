namespace tachyon {

template <typename Entity>
struct TacStorePlan;

class TacStore {
public:
    template <typename Result>
    static Result query(const json::JsonNode& plan, const std::vector<json::JsonNode>& arguments) {
        if (arguments.size() > 32) {
            throw TacBehaviorData::invalid();
        }
        for (const json::JsonNode& argument : arguments) {
            TacBehaviorData::bounded(argument, 1048576);
        }
        std::string operation = plan["operation"].getValue<std::string>();
        std::vector<bool> used(arguments.size(), false);
        json::JsonObject query;
        query["kind"] = operation == "findOne" ? std::string("find") : operation;
        query["where"] = predicates(plan, arguments, used);
        query["orderBy"] = plan["orderBy"];
        if (operation == "find") {
            query["page"] = page(plan, arguments, used);
        }
        if (operation == "findOne") {
            query["page"] = first();
        }
        if (std::find(used.begin(), used.end(), false) != used.end()) {
            throw TacBehaviorData::invalid();
        }
        json::JsonNode command = TacStore::command(plan["entity"], "query");
        command["query"] = query;
        json::JsonNode answer = run(command);
        check(operation, query, answer);
        if (operation != "findOne") {
            return json::JsonCodec<Result>::fromJson(answer);
        }
        if (answer.count() == 0) {
            return json::JsonCodec<Result>::fromJson(json::JsonNode(nullptr));
        }
        return json::JsonCodec<Result>::fromJson(answer[std::size_t{0}]);
    }

    static json::JsonNode save(const json::JsonNode& entity, const json::JsonNode& value) {
        TacBehaviorData::bounded(value, 1048576);
        if (value.getValueKind() != json::JsonValueKind::Object) {
            throw TacBehaviorData::invalid();
        }
        identifier(value[entity["id"].getValue<std::string>()]);
        json::JsonNode command = TacStore::command(entity, "save");
        command["value"] = value;
        return run(command);
    }

private:
    static json::JsonNode predicates(const json::JsonNode& plan, const std::vector<json::JsonNode>& arguments, std::vector<bool>& used) {
        json::JsonArray where;
        for (const json::JsonNode& predicate : plan["where"].items()) {
            std::string operation = predicate["operator"].getValue<std::string>();
            json::JsonObject written;
            written["field"] = predicate["field"];
            written["operator"] = operation;
            if (operation != "isNull" && operation != "isNotNull") {
                const json::JsonNode& value = arguments[argument(predicate["argument"], used)];
                if (!operand(operation, value)) {
                    throw TacBehaviorData::invalid();
                }
                written["value"] = value;
            }
            where.add(written);
        }
        return where;
    }

    static std::size_t argument(const json::JsonNode& written, std::vector<bool>& used) {
        std::size_t index = static_cast<std::size_t>(TacBehaviorData::integer(written, 32, 0, 31));
        if (index >= used.size() || used[index]) {
            throw TacBehaviorData::invalid();
        }
        used[index] = true;
        return index;
    }

    static bool operand(const std::string& operation, const json::JsonNode& value) {
        if (operation != "in") {
            return scalar(value);
        }
        if (value.getValueKind() != json::JsonValueKind::Array || value.count() > 100) {
            return false;
        }
        return std::all_of(value.items().begin(), value.items().end(), scalar);
    }

    static bool scalar(const json::JsonNode& value) {
        json::JsonValueKind kind = value.getValueKind();
        if (kind == json::JsonValueKind::Number) {
            return std::isfinite(value.getValue<double>());
        }
        return kind == json::JsonValueKind::Null || kind == json::JsonValueKind::True
            || kind == json::JsonValueKind::False || kind == json::JsonValueKind::String;
    }

    static json::JsonNode page(const json::JsonNode& plan, const std::vector<json::JsonNode>& arguments, std::vector<bool>& used) {
        json::JsonNode written = json::JsonObject();
        if (TacBehaviorData::present(plan["pageArgument"])) {
            written = arguments[argument(plan["pageArgument"], used)];
        }
        json::JsonObject policy;
        policy["defaultSize"] = plan["defaultSize"];
        policy["maxSize"] = plan["maxSize"];
        policy["sort"] = plan["fields"];
        json::JsonNode page = TacPageRequest::normalized(written, policy);
        if (page["sort"].count() != 0) {
            throw TacBehaviorData::invalid();
        }
        return page;
    }

    static json::JsonNode first() {
        json::JsonObject page;
        page["page"] = 0;
        page["size"] = 1;
        page["offset"] = 0;
        page["sort"] = json::JsonArray();
        return page;
    }

    static void check(const std::string& operation, const json::JsonNode& query, const json::JsonNode& answer) {
        if (operation == "find" || operation == "findOne") {
            long long size = TacBehaviorData::integer(query["page"]["size"], 0, 1, 100);
            bool rows = answer.getValueKind() == json::JsonValueKind::Array && static_cast<long long>(answer.count()) <= size;
            if (!rows) {
                throw TacBehaviorData::invalid();
            }
            for (const json::JsonNode& row : answer.items()) {
                if (row.getValueKind() != json::JsonValueKind::Object) {
                    throw TacBehaviorData::invalid();
                }
            }
            return;
        }
        if (operation == "exists") {
            json::JsonValueKind kind = answer.getValueKind();
            if (kind != json::JsonValueKind::True && kind != json::JsonValueKind::False) {
                throw TacBehaviorData::invalid();
            }
            return;
        }
        if (!TacBehaviorData::present(answer)) {
            throw TacBehaviorData::invalid();
        }
        TacBehaviorData::integer(answer, 0, 0, 9007199254740991LL);
    }

    static void identifier(const json::JsonNode& id) {
        json::JsonValueKind kind = id.getValueKind();
        if (kind == json::JsonValueKind::String) {
            std::string text = id.getValue<std::string>();
            if (text.empty() || text.size() > 256) {
                throw TacBehaviorData::invalid();
            }
            return;
        }
        if (kind != json::JsonValueKind::Number) {
            throw TacBehaviorData::invalid();
        }
        TacBehaviorData::integer(id, 0, -9007199254740991LL, 9007199254740991LL);
    }

    static json::JsonNode command(const json::JsonNode& entity, const char* operation) {
        json::JsonObject command;
        command["operation"] = operation;
        command["entity"] = entity["name"];
        command["table"] = entity["table"];
        command["idField"] = entity["id"];
        command["value"] = nullptr;
        command["page"] = nullptr;
        command["id"] = nullptr;
        command["query"] = nullptr;
        const std::string& transaction = TacTransactions::open(entity["adapter"].getValue<std::string>());
        if (!transaction.empty()) {
            command["transaction"] = transaction;
        }
        return command;
    }

    static json::JsonNode run(const json::JsonNode& command) {
        json::JsonObject request;
        request["op"] = "entity";
        request["command"] = command;
        TacBehaviorData::bounded(request, 1048576);
        json::JsonNode answer = TacBehaviorData::ask(request);
        TacBehaviorData::bounded(answer, 1048576);
        return answer;
    }
};

template <typename Entity, typename Id>
class Repository {
public:
    std::optional<Entity> findById(const Id& id) {
        return TacStore::query<std::optional<Entity>>(TacStorePlan<Entity>::findById(), {json::JsonCodec<Id>::toJson(id)});
    }

    std::vector<Entity> findAll() {
        return findAll(json::JsonObject());
    }

    std::vector<Entity> findAll(const json::JsonNode& page) {
        return TacStore::query<std::vector<Entity>>(TacStorePlan<Entity>::findAll(), {page});
    }

    long long deleteById(const Id& id) {
        return TacStore::query<long long>(TacStorePlan<Entity>::deleteById(), {json::JsonCodec<Id>::toJson(id)});
    }

    Entity save(const Entity& entity) {
        const json::JsonNode& plan = TacStorePlan<Entity>::findById();
        return json::JsonCodec<Entity>::fromJson(TacStore::save(plan["entity"], json::JsonCodec<Entity>::toJson(entity)));
    }
};

}
