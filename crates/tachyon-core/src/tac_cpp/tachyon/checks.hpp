namespace tachyon {

class TacValidationError : public TacCodedError {
public:
    explicit TacValidationError(const std::string& field) : TacCodedError("Behavior validation failed: " + field + ".", "validation"), failedPath(field) {}

    const std::string& path() const {
        return failedPath;
    }

private:
    std::string failedPath;
};

template <typename Model>
class TacChecks {
public:
    static void check(const Model&) {}
};

template <typename Value>
class TacMaybe {
public:
    static constexpr bool HELD = false;
};

template <typename Value>
class TacMaybe<std::optional<Value>> {
public:
    static constexpr bool HELD = true;
};

class TacCheck {
public:
    template <typename Value>
    static void required(const Value& value, const char* field) {
        if (missing(value)) {
            fail(field, "notNull");
        }
    }

    template <typename Value>
    static void notBlank(const Value& value, const char* field) {
        if (missing(value) || blank(held(value))) {
            fail(field, "notBlank");
        }
    }

    template <typename Value>
    static void size(const Value& value, std::size_t minimum, std::size_t maximum, const char* field) {
        if (missing(value)) {
            return;
        }
        std::size_t count = held(value).size();
        if (count < minimum || count > maximum) {
            fail(field, "size");
        }
    }

    template <typename Value>
    static void min(const Value& value, double bound, const char* field) {
        if (!missing(value) && static_cast<double>(held(value)) < bound) {
            fail(field, "min");
        }
    }

    template <typename Value>
    static void max(const Value& value, double bound, const char* field) {
        if (!missing(value) && static_cast<double>(held(value)) > bound) {
            fail(field, "max");
        }
    }

private:
    template <typename Value>
    static bool missing(const Value& value) {
        if constexpr (TacMaybe<Value>::HELD) {
            return !value.has_value();
        } else if constexpr (std::is_pointer_v<Value>) {
            return value == nullptr;
        } else {
            return false;
        }
    }

    template <typename Value>
    static const auto& held(const Value& value) {
        if constexpr (TacMaybe<Value>::HELD || std::is_pointer_v<Value>) {
            return *value;
        } else {
            return value;
        }
    }

    static bool blank(const std::string& text) {
        std::size_t at = 0;
        while (at < text.size()) {
            if (!isSpace(next(text, at))) {
                return false;
            }
        }
        return true;
    }

    static std::uint32_t next(const std::string& text, std::size_t& at) {
        unsigned char first = static_cast<unsigned char>(text[at]);
        std::size_t length = first < 0x80 ? 1 : first < 0xE0 ? 2 : first < 0xF0 ? 3 : 4;
        std::uint32_t point = length == 1 ? first : first & (0xFF >> (length + 1));
        for (std::size_t index = 1; index < length && at + index < text.size(); index += 1) {
            point = (point << 6) | (static_cast<unsigned char>(text[at + index]) & 0x3F);
        }
        at += length;
        return point;
    }

    static bool isSpace(std::uint32_t point) {
        bool control = point >= 0x09 && point <= 0x0D;
        bool spaces = point >= 0x2000 && point <= 0x200A;
        bool single = point == 0x20 || point == 0x85 || point == 0xA0 || point == 0x1680 || point == 0x2028 || point == 0x2029 || point == 0x202F || point == 0x205F || point == 0x3000;
        return control || spaces || single;
    }

    [[noreturn]] static void fail(const char* field, const char* rule) {
        throw TacValidationError(std::string(field) + "." + rule);
    }
};

}
