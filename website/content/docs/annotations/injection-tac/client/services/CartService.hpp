#pragma once
#include "../helpers/CartHelper.hpp"
#include "../helpers/CartPricesHelper.hpp"

class [[Service, Lifetime({"value":"prototype"})]] CartService {
public:
    [[Needs]] CartPricesHelper& cartPricesHelper;
    [[Needs]] CartHelper& cartHelper;

    long long total() const {
        return cartHelper.settle(cartPricesHelper.subtotal());
    }
};
