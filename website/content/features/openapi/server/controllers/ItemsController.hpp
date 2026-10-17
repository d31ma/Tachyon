#pragma once
#include <cstdint>
#include "../models/Item.hpp"

class [[RequestRoute("/items")]] [[Public]] ItemsController {
public:
    [[GetRoute("/{id}")]]
    Item getItem([[FromPath]] std::int64_t id) {
        return Item{"anvil", id};
    }

    [[PatchRoute("/{id}")]]
    Item patchItem([[FromPath]] std::int64_t id, [[FromBody]] Item item) {
        return Item{item.name, id};
    }
};
