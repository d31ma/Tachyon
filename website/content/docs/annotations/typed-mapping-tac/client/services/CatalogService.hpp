#pragma once
#include <string>
#include "../converters/ProductConverter.hpp"

class [[Service]] CatalogService {
public:
    std::string result;
    [[Needs]] ProductConverter& productConverter;

    void load() {
        Detail detail{"detail", "detail-private"};
        Product product{7, "Anvil", detail, {detail}, "product-private"};
        Summary summary = productConverter.map(productConverter.copy(product));
        result = summary.kind + " " + std::to_string(summary.id) + ": " + summary.name + ", " + summary.detail.name + ", " + std::to_string(summary.items.size());
    }
};
