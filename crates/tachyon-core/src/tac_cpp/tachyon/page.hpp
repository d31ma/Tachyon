extern "C" {
__attribute__((import_module("tachyon"), import_name("tac_host_emit"))) void tac_host_emit(const char* text, int length);
__attribute__((import_module("tachyon"), import_name("tac_host_storage"))) char* tac_host_storage(const char* text, int length);
__attribute__((import_module("tachyon"), import_name("tac_host_web"))) char* tac_host_web(const char* text, int length);
__attribute__((import_module("tachyon"), import_name("tac_host_epoch_ms"))) double tac_host_epoch_ms();
__attribute__((import_module("tachyon"), import_name("tac_host_http_start"))) void tac_host_http_start(std::uint32_t ticket, const char* text, int length);
__attribute__((import_module("tachyon"), import_name("tac_host_task_done"))) void tac_host_task_done(std::uint32_t task, const char* text, int length);
__attribute__((import_module("tachyon"), import_name("tac_host_http_cancel"))) void tac_host_http_cancel(std::uint32_t ticket);
}

namespace tachyon {

class TacHost {
public:
    static void emit(const std::string& text) {
        tac_host_emit(text.data(), static_cast<int>(text.size()));
    }

    static std::string storage(const std::string& request) {
        return answered(tac_host_storage(request.data(), static_cast<int>(request.size())), "Tachyon storage is not on this page.");
    }

    static std::string web(const std::string& request) {
        return answered(tac_host_web(request.data(), static_cast<int>(request.size())), "Tachyon browser APIs are not on this page.");
    }

    static double epochMilliseconds() {
        return tac_host_epoch_ms();
    }

    static void httpStart(std::uint32_t ticket, const std::string& request) {
        tac_host_http_start(ticket, request.data(), static_cast<int>(request.size()));
    }

    static void httpCancel(std::uint32_t ticket) {
        tac_host_http_cancel(ticket);
    }

    static void taskDone(std::uint32_t task, const std::string& answer) {
        tac_host_task_done(task, answer.data(), static_cast<int>(answer.size()));
    }

private:
    static std::string answered(char* answer, const char* missing) {
        if (answer == nullptr) {
            json::JsonObject failure;
            failure["error"] = missing;
            return failure.toJsonString();
        }
        std::string text(answer);
        std::free(answer);
        return text;
    }
};

}

namespace tachyon {

class TacMember {
public:
    std::string name;
    std::function<json::JsonNode(void*)> read;
    std::function<void(void*, const json::JsonNode&)> write;
};

class TacMethod {
public:
    std::string name;
    std::function<json::JsonNode(void*, const json::JsonNode&)> call;
    bool asynchronous = false;
};

class TacServiceType {
public:
    json::JsonNode metadata;
    std::function<std::shared_ptr<void>()> create;
    std::vector<TacMember> fields;
    std::vector<TacMember> props;
    std::vector<TacMethod> methods;
    std::vector<TacMethod> hooks;

    const TacMember* field(const std::string& name) const {
        for (const TacMember& member : fields) {
            if (member.name == name) {
                return &member;
            }
        }
        return nullptr;
    }

    const TacMethod* method(const std::string& name) const {
        return find(methods, name);
    }

    const TacMethod* hook(const std::string& name) const {
        return find(hooks, name);
    }

private:
    static const TacMethod* find(const std::vector<TacMethod>& list, const std::string& name) {
        for (const TacMethod& method : list) {
            if (method.name == name) {
                return &method;
            }
        }
        return nullptr;
    }
};

class TacProgram {
public:
    static std::map<std::string, TacServiceType> services();
};

}

namespace tachyon {

class TacEvent {
public:
    std::string name;
    std::string value;
    bool retain = false;
};

class TacOwner {
public:
    std::string route;
    std::string service;
    std::string id;
    std::shared_ptr<void> instance;
    const TacServiceType* type = nullptr;
    std::deque<TacEvent> queue;
    bool disposed = false;

    bool matches(const json::JsonNode& request) const {
        return route == request["route"].getValue<std::string>() && service == request["service"].getValue<std::string>();
    }

    void runHooks(const char* list) {
        const json::JsonNode& names = type->metadata[list];
        if (names.getValueKind() != json::JsonValueKind::Array) {
            return;
        }
        for (const json::JsonNode& name : names.items()) {
            const TacMethod* hook = type->hook(name.getValue<std::string>());
            if (hook != nullptr) {
                hook->call(instance.get(), json::JsonArray());
            }
        }
    }
};

}

namespace tachyon {

class TacPage {
public:
    static std::string invoke(const std::string& text) {
        TacPage& page = current();
        if (page.handling) {
            return failure("Native owner is busy.");
        }
        page.handling = true;
        std::string answer = page.answered(text);
        page.executing.clear();
        page.handling = false;
        return answer;
    }

    static std::string executingOwner() {
        return current().executing;
    }

    static void resumeFor(const std::string& owner, std::coroutine_handle<> waiting) {
        TacPage& page = current();
        std::string before = page.executing;
        page.executing = owner;
        waiting.resume();
        page.drain();
        page.executing = before;
    }

    static void finishTask(std::uint32_t task, const std::string& owner, const std::string& answer) {
        TacPage& page = current();
        std::string before = page.executing;
        page.executing = owner;
        page.drain();
        page.executing = before;
        TacHost::taskDone(task, answer);
    }

    static void publish(const std::string& name, const json::JsonNode& value, bool retain) {
        TacPage& page = current();
        TacOwner* owner = page.owner(page.executing);
        std::string encoded = value.toJsonString();
        bool fits = encoded.size() <= MAX_EVENT_BYTES;
        if (owner == nullptr || owner->disposed || !fits || owner->queue.size() >= MAX_QUEUED_EVENTS) {
            std::fputs("Tachyon native event delivery failed.\n", stderr);
            return;
        }
        owner->queue.push_back(TacEvent{name, encoded, retain});
    }

private:
    static constexpr std::size_t MAX_OWNERS = 256;
    static constexpr std::size_t MAX_QUEUED_EVENTS = 128;
    static constexpr std::size_t MAX_MEMBER_REQUEST_BYTES = 65536;
    static constexpr std::size_t MAX_EVENT_BYTES = 65536;
    static constexpr std::size_t MAX_RETURNED_EVENTS = 32;
    static constexpr std::size_t OWNER_LENGTH = 36;

    std::map<std::string, TacServiceType> services = TacProgram::services();
    std::map<std::string, TacOwner> owners;
    std::string executing;
    bool handling = false;

    static TacPage& current() {
        static TacPage page;
        return page;
    }

    static std::string answer(const json::JsonNode& value) {
        json::JsonObject answered;
        answered["value"] = value;
        return answered.toJsonString();
    }

    static std::string started(const json::JsonNode& task) {
        json::JsonObject answered;
        answered["task"] = task;
        return answered.toJsonString();
    }

    static std::string failure(const std::string& message) {
        json::JsonObject answered;
        answered["error"] = message;
        return answered.toJsonString();
    }

    static std::string coded(const std::string& message, const std::string& code) {
        json::JsonObject answered;
        answered["error"] = message;
        answered["code"] = code;
        return answered.toJsonString();
    }

    static bool present(const json::JsonNode& value) {
        return value.getValueKind() == json::JsonValueKind::String && !value.getValue<std::string>().empty();
    }

    static bool isOwnerId(const json::JsonNode& value) {
        if (value.getValueKind() != json::JsonValueKind::String) {
            return false;
        }
        std::string id = value.getValue<std::string>();
        if (id.size() != OWNER_LENGTH) {
            return false;
        }
        for (std::size_t index = 0; index < id.size(); index += 1) {
            bool dash = index == 8 || index == 13 || index == 18 || index == 23;
            if (dash != (id[index] == '-')) {
                return false;
            }
            if (!dash && !std::isxdigit(static_cast<unsigned char>(id[index]))) {
                return false;
            }
        }
        return true;
    }

    TacOwner* owner(const std::string& id) {
        auto found = owners.find(id);
        return found == owners.end() ? nullptr : &found->second;
    }

    std::string answered(const std::string& text) {
        try {
            std::string result = handle(text);
            drain();
            return result;
        } catch (const std::exception& problem) {
            return failure(problem.what());
        } catch (...) {
            return failure("The C++ service failed.");
        }
    }

    std::string handle(const std::string& text) {
        json::JsonNode request;
        try {
            request = json::JsonNode::parse(text);
        } catch (const json::JsonException&) {
            return failure("Invalid companion request.");
        }
        if (request.getValueKind() != json::JsonValueKind::Object || !present(request["op"])) {
            return failure("Companion request has no operation.");
        }
        std::string operation = request["op"].getValue<std::string>();
        if (operation != "init" && text.size() > MAX_MEMBER_REQUEST_BYTES) {
            return failure("The request is " + std::to_string(text.size()) + " bytes. The limit is 65536 bytes. Pass less data to the service.");
        }
        if (operation == "host-dispose") {
            disposeAll();
            return answer(json::JsonNode());
        }
        std::string problem = identityProblem(request);
        if (!problem.empty()) {
            return problem;
        }
        return handleOwned(request, operation);
    }

    static std::string identityProblem(const json::JsonNode& request) {
        if (!present(request["route"])) {
            return coded("Missing companion route.", "TY_NATIVE_ROUTE");
        }
        if (!present(request["service"])) {
            return coded("Missing companion name.", "TY_NATIVE_SERVICE");
        }
        if (!isOwnerId(request["owner"])) {
            return coded("Missing native owner identity.", "TY_NATIVE_OWNER");
        }
        return std::string();
    }

    std::string handleOwned(const json::JsonNode& request, const std::string& operation) {
        std::string id = request["owner"].getValue<std::string>();
        if (operation == "init") {
            std::string failed = activate(request, id);
            if (!failed.empty()) {
                return failed;
            }
        }
        TacOwner* found = owner(id);
        if (found == nullptr || !found->matches(request)) {
            return coded("Stale native owner.", "TY_NATIVE_OWNER");
        }
        if (operation == "dispose") {
            disposeOwner(id);
            return answer(json::JsonNode());
        }
        executing = id;
        if (operation == "init") {
            return answer(found->type->metadata["page"]);
        }
        if (!present(request["name"])) {
            return failure("Companion request has no member name.");
        }
        std::string name = request["name"].getValue<std::string>();
        if (operation == "event") {
            return receive(*found, name, request["value"]);
        }
        return useMember(*found, operation, name, request);
    }

    std::string activate(const json::JsonNode& request, const std::string& id) {
        TacOwner* existing = owner(id);
        if (existing != nullptr && existing->matches(request)) {
            return std::string();
        }
        if (existing != nullptr) {
            disposeOwner(id);
        }
        auto registered = services.find(request["service"].getValue<std::string>());
        if (registered == services.end()) {
            return coded("Unknown native service.", "TY_NATIVE_SERVICE");
        }
        if (owners.size() >= MAX_OWNERS) {
            return failure("Too many native activations.");
        }
        TacOwner created;
        created.route = request["route"].getValue<std::string>();
        created.service = registered->first;
        created.id = id;
        created.type = &registered->second;
        created.instance = registered->second.create();
        receiveProps(created, request["props"]);
        owners[id] = created;
        executing = id;
        return start(owners[id]);
    }

    static void receiveProps(TacOwner& created, const json::JsonNode& given) {
        if (given.getValueKind() != json::JsonValueKind::Object) {
            return;
        }
        for (const TacMember& prop : created.type->props) {
            if (given.containsKey(prop.name)) {
                prop.write(created.instance.get(), given[prop.name]);
            }
        }
    }

    std::string start(TacOwner& started) {
        try {
            started.runHooks("starts");
            drain();
            return std::string();
        } catch (const std::exception& problem) {
            std::string message = problem.what();
            disposeOwner(started.id);
            return failure("Native initializer failed: " + message);
        }
    }

    std::string receive(TacOwner& receiver, const std::string& name, const json::JsonNode& value) {
        const json::JsonNode* listener = listenerNamed(receiver, name);
        const TacMethod* hook = receiver.type->hook(name);
        if (listener == nullptr || hook == nullptr) {
            return failure("Unknown companion listener: " + name);
        }
        json::JsonArray values;
        values.add(value);
        json::JsonNode result = hook->call(receiver.instance.get(), values);
        bool background = (*listener)["asynchronous"].getValueKind() == json::JsonValueKind::True;
        if (!result.isNull() && !background) {
            republish(result, (*listener)["returns"]);
        }
        return answer(json::JsonNode());
    }

    static const json::JsonNode* listenerNamed(const TacOwner& receiver, const std::string& name) {
        const json::JsonNode& listeners = receiver.type->metadata["listeners"];
        if (listeners.getValueKind() != json::JsonValueKind::Array) {
            return nullptr;
        }
        for (const json::JsonNode& listener : listeners.items()) {
            if (listener["method"].getValue<std::string>() == name) {
                return &listener;
            }
        }
        return nullptr;
    }

    static void republish(const json::JsonNode& result, const json::JsonNode& returns) {
        if (!present(returns)) {
            return;
        }
        std::string name = returns.getValue<std::string>();
        if (result.getValueKind() != json::JsonValueKind::Array) {
            publish(name, result, false);
            return;
        }
        std::size_t limit = std::min(result.count(), MAX_RETURNED_EVENTS);
        for (std::size_t index = 0; index < limit; index += 1) {
            publish(name, result[index], false);
        }
    }

    std::string useMember(TacOwner& used, const std::string& operation, const std::string& name, const json::JsonNode& request) {
        const TacMember* field = used.type->field(name);
        const TacMethod* method = used.type->method(name);
        if (operation == "get" && field != nullptr) {
            return answer(field->read(used.instance.get()));
        }
        if (operation == "set" && field != nullptr) {
            field->write(used.instance.get(), request["value"]);
            return answer(json::JsonNode());
        }
        if (operation == "call" && method != nullptr && method->asynchronous) {
            return started(method->call(used.instance.get(), request["args"]));
        }
        if (operation == "call" && method != nullptr) {
            return answer(method->call(used.instance.get(), request["args"]));
        }
        if (field == nullptr && method == nullptr) {
            return failure("Unknown companion member: " + name);
        }
        return failure("Companion member does not support " + operation + ": " + name);
    }

    void drain() {
        TacOwner* sending = owner(executing);
        while (sending != nullptr && !sending->disposed && !sending->queue.empty()) {
            TacEvent event = sending->queue.front();
            sending->queue.pop_front();
            send(*sending, event);
        }
    }

    static void send(const TacOwner& sender, const TacEvent& event) {
        json::JsonObject signal;
        signal["route"] = sender.route;
        signal["service"] = sender.service;
        signal["owner"] = sender.id;
        signal["name"] = event.name;
        signal["value"] = json::JsonNode::parse(event.value);
        signal["retain"] = event.retain;
        TacHost::emit(signal.toJsonString());
    }

    void disposeOwner(const std::string& id) {
        auto found = owners.find(id);
        if (found == owners.end()) {
            return;
        }
        TacOwner removed = found->second;
        owners.erase(found);
        if (executing == id) {
            executing.clear();
        }
        removed.disposed = true;
        try {
            removed.runHooks("stops");
        } catch (const std::exception&) {
            std::fputs("Tachyon native disposal failed.\n", stderr);
        }
    }

    void disposeAll() {
        std::vector<std::string> ids;
        for (const auto& [id, value] : owners) {
            ids.push_back(id);
        }
        for (const std::string& id : ids) {
            disposeOwner(id);
        }
    }
};

}

namespace tachyon {

class TacTaskOwner {
public:
    static std::string current() {
        return TacPage::executingOwner();
    }

    static void resumeFor(const std::string& owner, std::coroutine_handle<> waiting) {
        TacPage::resumeFor(owner, waiting);
    }

    static void finish(std::uint32_t task, const std::string& owner, const std::string& answer) {
        TacPage::finishTask(task, owner, answer);
    }
};

}
