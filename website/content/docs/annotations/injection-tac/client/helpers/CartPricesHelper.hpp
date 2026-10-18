#pragma once

class [[Helper]] CartPricesHelper {
public:
    long long subtotal() const { return 40; }
    long long surcharge() const { return 2; }
    long long count() const { return 3; }
};
