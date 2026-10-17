#pragma once
#include <cstdint>
#include "../models/Item.hpp"

class [[RequestRoute("/items")]] [[Public]] ItemsController {
public:
    [[GetRoute("/{id}")]]
    Item getItem([[FromPath]] std::int64_t id) {
        Item item;
        item.id = id;
        return item;
    }
};
