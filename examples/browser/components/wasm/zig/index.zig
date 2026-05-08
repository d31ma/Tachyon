const std = @import("std");

const prefix = "{\"state\":{\"clicks\":";
const suffix = ",\"label\":\"Zig\"}}";

var input: [2048]u8 = undefined;
var output: [128]u8 = undefined;
var output_len_value: usize = 0;
var clicks: u32 = 0;

fn writeState() void {
    var pos: usize = 0;
    pos = copyText(pos, prefix);
    pos = writeUint(pos, clicks);
    pos = copyText(pos, suffix);
    output_len_value = pos;
}

fn copyText(pos_value: usize, text: []const u8) usize {
    var pos = pos_value;
    for (text) |byte| {
        output[pos] = byte;
        pos += 1;
    }
    return pos;
}

fn writeUint(pos_value: usize, value: u32) usize {
    var digits: [10]u8 = undefined;
    var cursor: usize = digits.len;
    var current = value;
    while (true) {
        cursor -= 1;
        digits[cursor] = @intCast('0' + (current % 10));
        current /= 10;
        if (current == 0) break;
    }
    var pos = pos_value;
    while (cursor < digits.len) : (cursor += 1) {
        output[pos] = digits[cursor];
        pos += 1;
    }
    return pos;
}

fn readClicks(payload: []const u8) u32 {
    const marker = "\"clicks\":";
    var index: usize = 0;
    while (index + marker.len <= payload.len) : (index += 1) {
        if (std.mem.eql(u8, payload[index..index + marker.len], marker)) {
            var cursor = index + marker.len;
            var value: u32 = 0;
            while (cursor < payload.len and payload[cursor] >= '0' and payload[cursor] <= '9') : (cursor += 1) {
                value = value * 10 + @as(u32, payload[cursor] - '0');
            }
            return value;
        }
    }
    return 0;
}

export fn alloc(size: usize) [*]u8 {
    _ = size;
    return &input;
}

export fn dealloc(ptr: [*]u8, len: usize) void {
    _ = ptr;
    _ = len;
}

export fn init(ptr: [*]const u8, len: usize) void {
    _ = ptr;
    _ = len;
    clicks = 0;
    writeState();
}

export fn call(method_ptr: [*]const u8, method_len: usize, payload_ptr: [*]const u8, payload_len: usize) void {
    _ = method_ptr;
    _ = method_len;
    _ = payload_ptr;
    clicks = readClicks(input[0..payload_len]) + 1;
    writeState();
}

export fn output_ptr() usize {
    return @intFromPtr(&output);
}

export fn output_len() usize {
    return output_len_value;
}
