#pragma once
#include <string>
#include <vector>
#include "ShortDetail.hpp"

struct Summary {
    long long id = 0;
    std::string name;
    ShortDetail detail;
    std::vector<ShortDetail> items;
    std::string kind;
};
