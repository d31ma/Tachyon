#pragma once
#include <optional>
#include <string>

struct [[Checked]] OrderInput {
    [[NotEmpty, Length({"min":1,"max":80})]] std::string name;
    [[Required, Min(1), Max(9)]] std::optional<double> level;
};
