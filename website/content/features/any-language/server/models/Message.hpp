#pragma once
#include <cstdint>
#include <string>

struct Message {
    std::string name;
    std::int64_t level = 0;
};
