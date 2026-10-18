namespace tachyon {

template <typename Settings>
struct TacSettingsPlan;

template <typename Model>
struct TacValuesPlan;

class TacSettingsBinder {
public:
    using Values = std::map<std::string, std::string>;

    static json::JsonNode properties(const json::JsonNode& plan, const Values& values) {
        check(values);
        std::string prefix = plan["prefix"].getValue<std::string>();
        const json::JsonNode& fields = plan["fields"];
        bool shaped = name(prefix) && prefix.size() <= 128 && prefix.rfind("TAC_", 0) == 0 && prefix.back() == '_'
            && fields.getValueKind() == json::JsonValueKind::Object && !fields.propertyNames().empty() && fields.propertyNames().size() <= 64;
        if (!shaped) {
            throw TacBehaviorData::invalid();
        }
        std::vector<std::string> names;
        json::JsonObject output;
        for (const std::string& field : fields.propertyNames()) {
            std::string full = prefix + fields[field]["name"].getValue<std::string>();
            if (!name(full) || std::find(names.begin(), names.end(), full) != names.end()) {
                throw TacBehaviorData::invalid();
            }
            names.push_back(full);
            auto found = values.find(full);
            output[field] = value(fields[field], found == values.end() ? nullptr : &found->second);
        }
        for (const auto& [key, written] : values) {
            if (key.rfind(prefix, 0) == 0 && std::find(names.begin(), names.end(), key) == names.end()) {
                throw TacBehaviorData::invalid();
            }
        }
        TacBehaviorData::bounded(output, 65536);
        return output;
    }

    static json::JsonNode values(const json::JsonNode& plan, const json::JsonNode& application) {
        const json::JsonNode& fields = plan["fields"];
        if (fields.propertyNames().empty() || fields.propertyNames().size() > 64) {
            throw TacBehaviorData::invalid();
        }
        Values flat = flattened(application);
        json::JsonObject output;
        for (const std::string& field : fields.propertyNames()) {
            std::string path = fields[field]["path"].getValue<std::string>();
            for (const auto& [key, written] : flat) {
                if (key.rfind(path + ".", 0) == 0) {
                    throw TacBehaviorData::invalid();
                }
            }
            auto found = flat.find(path);
            output[field] = value(fields[field], found == flat.end() ? nullptr : &found->second);
        }
        TacBehaviorData::bounded(output, 65536);
        return output;
    }

    static Values fromApplication(const json::JsonNode& application) {
        Values inputs;
        for (const auto& [path, written] : flattened(application)) {
            if (path.rfind("tac.", 0) != 0) {
                continue;
            }
            std::string key = path;
            for (char& character : key) {
                character = character == '.' ? '_' : static_cast<char>(std::toupper(static_cast<unsigned char>(character)));
            }
            inputs[key] = written;
        }
        return inputs;
    }

private:
    static bool name(const std::string& text) {
        if (text.empty() || text.size() > 256 || !std::isupper(static_cast<unsigned char>(text[0]))) {
            return false;
        }
        return std::all_of(text.begin(), text.end(), [](char character) {
            return std::isupper(static_cast<unsigned char>(character)) || std::isdigit(static_cast<unsigned char>(character)) || character == '_';
        });
    }

    static void check(const Values& values) {
        if (values.size() > 256) {
            throw TacBehaviorData::invalid();
        }
        json::JsonObject written;
        for (const auto& [key, text] : values) {
            if (!name(key) || key.rfind("TAC_", 0) != 0 || text.size() > 4096) {
                throw TacBehaviorData::invalid();
            }
            written[key] = text;
        }
        TacBehaviorData::bounded(written, 65536);
    }

    static bool whole(double number, bool integer) {
        return std::isfinite(number) && std::fabs(number) <= 9007199254740991.0 && (!integer || number == std::trunc(number));
    }

    static json::JsonNode value(const json::JsonNode& field, const std::string* raw) {
        std::string kind = field["type"].getValue<std::string>();
        bool numeric = kind == "integer" || kind == "number";
        bool integer = kind == "integer";
        bool known = kind == "string" || kind == "boolean" || numeric;
        if (!known) {
            throw TacBehaviorData::invalid();
        }
        for (const char* bound : {"min", "max"}) {
            const json::JsonNode& limit = field[bound];
            if (TacBehaviorData::present(limit) && (!numeric || limit.getValueKind() != json::JsonValueKind::Number || !whole(limit.getValue<double>(), integer))) {
                throw TacBehaviorData::invalid();
            }
        }
        json::JsonNode chosen = raw != nullptr ? parsed(kind, *raw, integer) : field["default"];
        if (!TacBehaviorData::present(chosen) || !allowed(field, kind, chosen, integer)) {
            throw TacBehaviorData::invalid();
        }
        return chosen;
    }

    static json::JsonNode parsed(const std::string& kind, const std::string& raw, bool integer) {
        if (kind == "string") {
            return raw;
        }
        if (kind == "boolean") {
            if (raw != "true" && raw != "false") {
                throw TacBehaviorData::invalid();
            }
            return raw == "true";
        }
        if (!token(raw, integer)) {
            throw TacBehaviorData::invalid();
        }
        return std::strtod(raw.c_str(), nullptr);
    }

    static bool allowed(const json::JsonNode& field, const std::string& kind, const json::JsonNode& value, bool integer) {
        json::JsonValueKind written = value.getValueKind();
        if (written == json::JsonValueKind::String) {
            return kind == "string" && value.getValue<std::string>().size() <= 4096;
        }
        if (written == json::JsonValueKind::True || written == json::JsonValueKind::False) {
            return kind == "boolean";
        }
        if (written != json::JsonValueKind::Number || kind == "string" || kind == "boolean") {
            return false;
        }
        double number = value.getValue<double>();
        bool aboveMin = !TacBehaviorData::present(field["min"]) || number >= field["min"].getValue<double>();
        bool belowMax = !TacBehaviorData::present(field["max"]) || number <= field["max"].getValue<double>();
        return whole(number, integer) && aboveMin && belowMax;
    }

    static bool token(const std::string& text, bool integer) {
        std::size_t position = text.rfind('-', 0) == 0 ? 1 : 0;
        if (position >= text.size() || !std::isdigit(static_cast<unsigned char>(text[position]))) {
            return false;
        }
        position = text[position] == '0' ? position + 1 : digits(text, position);
        if (!integer && position < text.size() && text[position] == '.') {
            std::size_t first = position + 1;
            position = digits(text, first);
            if (position == first) {
                return false;
            }
        }
        if (!integer && position < text.size() && (text[position] == 'e' || text[position] == 'E')) {
            position += 1;
            if (position < text.size() && (text[position] == '+' || text[position] == '-')) {
                position += 1;
            }
            std::size_t first = position;
            position = digits(text, first);
            if (position == first) {
                return false;
            }
        }
        return position == text.size();
    }

    static std::size_t digits(const std::string& text, std::size_t position) {
        while (position < text.size() && std::isdigit(static_cast<unsigned char>(text[position]))) {
            position += 1;
        }
        return position;
    }

    static Values flattened(const json::JsonNode& application) {
        TacBehaviorData::bounded(application, 65536);
        if (application.getValueKind() != json::JsonValueKind::Object) {
            throw TacBehaviorData::invalid();
        }
        Values flat;
        bool framed = false;
        for (const std::string& key : application.propertyNames()) {
            if (key != "tac" && key != "yon") {
                continue;
            }
            framed = true;
            if (application[key].getValueKind() != json::JsonValueKind::Object) {
                throw TacBehaviorData::invalid();
            }
            flatten(application[key], key, 0, flat);
        }
        if (!framed) {
            throw TacBehaviorData::invalid();
        }
        return flat;
    }

    static void flatten(const json::JsonNode& value, const std::string& path, int depth, Values& flat) {
        if (depth > 16 || path.size() > 256 || flat.size() >= 256) {
            throw TacBehaviorData::invalid();
        }
        json::JsonValueKind kind = value.getValueKind();
        bool port = kind == json::JsonValueKind::Number && whole(value.getValue<double>(), true);
        if (path == "yon.server.port" && !port) {
            throw TacBehaviorData::invalid();
        }
        if (kind != json::JsonValueKind::Object) {
            scalar(value, path, flat);
            return;
        }
        if (depth > 0 && value.propertyNames().empty()) {
            flat[path + "."] = "";
        }
        for (const std::string& key : value.propertyNames()) {
            bool plain = !key.empty() && std::islower(static_cast<unsigned char>(key[0])) && std::all_of(key.begin(), key.end(), [](char character) {
                return std::islower(static_cast<unsigned char>(character)) || std::isdigit(static_cast<unsigned char>(character));
            });
            if (!plain) {
                throw TacBehaviorData::invalid();
            }
            flatten(value[key], path + "." + key, depth + 1, flat);
        }
    }

    static void scalar(const json::JsonNode& value, const std::string& path, Values& flat) {
        json::JsonValueKind kind = value.getValueKind();
        if (kind == json::JsonValueKind::Null || kind == json::JsonValueKind::Array) {
            flat[path + "."] = "";
            return;
        }
        std::string text;
        if (kind == json::JsonValueKind::String) {
            text = value.getValue<std::string>();
        } else if (kind == json::JsonValueKind::True || kind == json::JsonValueKind::False) {
            text = kind == json::JsonValueKind::True ? "true" : "false";
        } else if (kind == json::JsonValueKind::Number && std::isfinite(value.getValue<double>())) {
            text = value.toJsonString();
        } else {
            throw TacBehaviorData::invalid();
        }
        if (text.size() > 4096 || flat.size() >= 256) {
            throw TacBehaviorData::invalid();
        }
        flat[path] = text;
    }
};

class Behaviors {
public:
    template <typename Settings>
    static Settings properties(const std::map<std::string, std::string>& values) {
        return bound<Settings>([&]() { return TacSettingsBinder::properties(TacSettingsPlan<Settings>::plan(), values); }, PROPERTIES);
    }

    template <typename Settings>
    static Settings properties(const json::JsonNode& application) {
        return bound<Settings>([&]() {
            return TacSettingsBinder::properties(TacSettingsPlan<Settings>::plan(), TacSettingsBinder::fromApplication(application));
        }, PROPERTIES);
    }

    template <typename Model>
    static Model values(const json::JsonNode& application) {
        return bound<Model>([&]() { return TacSettingsBinder::values(TacValuesPlan<Model>::plan(), application); }, VALUES);
    }

private:
    static constexpr const char* PROPERTIES = "Invalid or missing explicit configuration properties.";
    static constexpr const char* VALUES = "Invalid or missing application configuration value.";

    template <typename Model, typename Bind>
    static Model bound(Bind bind, const char* message) {
        try {
            return json::JsonCodec<Model>::fromJson(bind());
        } catch (const std::exception&) {
            throw TacCodedError(message, "configuration-properties");
        }
    }
};

}
