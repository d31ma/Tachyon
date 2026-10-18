#pragma once
#include <string>

class [[Service]] EditorService {
public:
    [[SessionState]] std::string draft;
    [[LocalState]] std::string theme = "calm";
};
