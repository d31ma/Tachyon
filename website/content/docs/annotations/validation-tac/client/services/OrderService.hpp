#pragma once
#include <string>
#include "../models/OrderInput.hpp"

class [[Service]] OrderService {
public:
    std::string status = "ready";

    OrderInput accept([[Check]] const OrderInput& order) { return order; }
};
