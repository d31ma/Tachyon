#pragma once
#include "../models/Message.hpp"

class [[RequestRoute("/echo")]] [[Public]] EchoController {
public:
    [[PostRoute]]
    Message postEcho([[FromBody]] Message message) {
        return message;
    }
};
