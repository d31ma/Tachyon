#pragma once
#include "../models/Greeting.hpp"

class [[RequestRoute("/api/greeting")]] [[Public]] GreetingController {
public:
    [[GetRoute]]
    Greeting getGreeting() {
        return Greeting{"Hello from Yon"};
    }
};
