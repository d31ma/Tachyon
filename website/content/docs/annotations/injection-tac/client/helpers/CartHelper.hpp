#pragma once
#include "CartPricesHelper.hpp"

class [[Helper]] CartHelper {
public:
    [[Needs]] CartPricesHelper& cartPricesHelper;

    long long settle(long long subtotal) const {
        return subtotal + cartPricesHelper.surcharge();
    }

    long long count() const { return cartPricesHelper.count(); }
};
