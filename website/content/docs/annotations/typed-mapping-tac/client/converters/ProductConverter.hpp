#pragma once
#include "../models/Product.hpp"
#include "../models/Summary.hpp"

class [[Converter]] ProductConverter {
public:
    [[Convert({"fields":[{"target":"kind","constant":"product"}]})]]
    Summary map(const Product& product);

    Product copy(const Product& product);
};
