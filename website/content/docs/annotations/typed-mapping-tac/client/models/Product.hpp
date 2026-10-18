#pragma once
#include <string>
#include <vector>
#include "Detail.hpp"

struct Product {
    long long id = 0;
    std::string name;
    Detail detail;
    std::vector<Detail> items;
    std::string secret;
};
