#pragma once
#include "../models/Guide.hpp"

class [[RequestRoute("/guide")]] [[Public]] GuideController {
public:
    [[GetRoute]]
    Guide getGuide() {
        return Guide{"Models hold types", 3};
    }
};
