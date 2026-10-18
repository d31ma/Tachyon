#pragma once
#include <cstdint>
#include <string>

struct [[Topic]] InventoryRecord {
    std::string sku;
    std::int64_t level = 0;
};
