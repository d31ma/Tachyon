#pragma once
#include <vector>
#include "../models/InventoryRecord.hpp"

class [[Service]] InventoryService {
public:
    std::vector<InventoryRecord> records;

    [[OnEvent]] void addRecord(const InventoryRecord& record) {
        if (records.size() >= 64) {
            records.erase(records.begin(), records.end() - 63);
        }
        records.push_back(record);
    }
};
