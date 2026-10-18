#pragma once
#include "../helpers/CartHelper.hpp"

class [[Service]] AuditService {
public:
    [[Needs, Named("CartHelper")]] CartHelper& ledger;

    long long lines() const { return ledger.count(); }
};
