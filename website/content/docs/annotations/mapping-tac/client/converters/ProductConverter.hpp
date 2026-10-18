#pragma once

class [[Converter]] ProductConverter {
public:
    [[Convert({"fields":[{"source":"detail.id","target":"productId"},{"source":"name","target":"title"},{"source":"missing.deep","target":"absent"},{"target":"kind","constant":"product"}]})]]
    tachyon::json::JsonNode toSummary(const tachyon::json::JsonNode& product);
};
