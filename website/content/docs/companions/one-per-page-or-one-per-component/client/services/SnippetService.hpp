#pragma once
#include <string>
#include <vector>

class [[Service, NeedsAll]] SnippetService {
public:
    std::vector<std::string> files;
};
