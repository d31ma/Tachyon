namespace tachyon {

class TacConvert {
public:
    template <typename Target, typename Source>
    static Target copy(const Source& value) {
        if constexpr (std::is_same_v<Target, Source>) {
            return value;
        } else if constexpr (std::is_arithmetic_v<Target> && std::is_arithmetic_v<Source>) {
            return static_cast<Target>(value);
        } else {
            return json::JsonCodec<Target>::fromJson(json::JsonCodec<Source>::toJson(value));
        }
    }

    template <typename Target>
    static Target read(const json::JsonNode& source, std::initializer_list<const char*> path) {
        return json::JsonCodec<Target>::fromJson(at(source, path));
    }

    template <typename Target>
    static Target constant(std::string_view written) {
        return json::JsonCodec<Target>::fromJson(json::JsonNode::parse(written));
    }

    static json::JsonNode at(const json::JsonNode& source, std::initializer_list<const char*> path) {
        const json::JsonNode* current = &source;
        for (const char* name : path) {
            bool present = current->getValueKind() == json::JsonValueKind::Object && current->containsKey(name);
            if (!present) {
                return json::JsonNode(nullptr);
            }
            current = &(*current)[name];
        }
        return *current;
    }
};

}
