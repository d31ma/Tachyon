#pragma once
#include <string>
#include "../converters/ProductConverter.hpp"

class [[Service]] CatalogService {
public:
    std::string title = "Not mapped.";
    [[Needs]] ProductConverter& productConverter;

    void load() {
        tachyon::json::JsonNode product = tachyon::json::JsonNode::parse(R"({"detail":{"id":7},"name":"Anvil","extra":"dropped"})");
        tachyon::json::JsonNode summary = productConverter.toSummary(product);
        title = summary["kind"].getValue<std::string>() + " " + summary["productId"].toJsonString() + ": " + summary["title"].getValue<std::string>();
    }
};
