// Shared by every generated Apple host, including applications without Swift
// companions. Foundation owns JSON semantics; this byte scan only enforces
// budgets and rejects duplicate root keys before Foundation would discard them.
import Foundation
import CoreFoundation

func tachyonParseJSONRequest(_ raw: String) -> [String: Any]? {
    guard raw.utf8.count <= 65536 else { return nil }
    let bytes = Array(raw.utf8)
    guard tachyonJSONStructureAllowed(bytes) else { return nil }
    return (try? JSONSerialization.jsonObject(with: Data(bytes))) as? [String: Any]
}

// Find the closing quote without interpreting Unicode or JSON values. Escaped
// quotes and backslashes remain inside the token for Foundation to decode.
private func tachyonJSONQuotedTokenEnd(_ bytes: [UInt8], start: Int) -> Int? {
    var index = start + 1
    while index < bytes.count {
        if bytes[index] == 34 { return index }
        if bytes[index] == 92 { index += 1 }
        index += 1
    }
    return nil
}

private func tachyonJSONRootKey(_ bytes: [UInt8], start: Int, end: Int) -> String? {
    var next = end + 1
    while next < bytes.count && [9, 10, 13, 32].contains(bytes[next]) { next += 1 }
    guard next < bytes.count, bytes[next] == 58 else { return nil }
    // Decode only the key token. "route" and "\u0072oute" must compare equally.
    // Invalid strings are refused by the final Foundation object parse too.
    return (try? JSONSerialization.jsonObject(
        with: Data(bytes[start...end]), options: [.fragmentsAllowed])) as? String
}

private func tachyonJSONStructureAllowed(_ bytes: [UInt8]) -> Bool {
    var index = 0
    var depth = 0
    var keys = Set<String>()
    while index < bytes.count {
        switch bytes[index] {
        case 123, 91:
            depth += 1
            if depth > 64 { return false }
        case 125, 93:
            depth -= 1
            if depth < 0 { return false }
        case 34:
            let start = index
            guard let end = tachyonJSONQuotedTokenEnd(bytes, start: start) else { return false }
            index = end
            if depth == 1, let key = tachyonJSONRootKey(bytes, start: start, end: end) {
                if !keys.insert(key).inserted { return false }
            }
        default: break
        }
        index += 1
    }
    return depth == 0
}

func tachyonCanonicalJSONRequest(_ request: [String: Any]) -> String? {
    guard let data = try? JSONSerialization.data(withJSONObject: request, options: [.sortedKeys]),
          data.count <= 65536 else { return nil }
    return String(data: data, encoding: .utf8)
}

// Native closures receive ordinary Swift scalars, not NSNumber's permissive
// bool/number casts. Preserve explicit nulls inside arrays and dictionaries.
func tachyonSwiftJSONValue(_ value: Any?) -> Any? {
    guard let value, !(value is NSNull) else { return nil }
    if let number = value as? NSNumber {
        if CFGetTypeID(number) == CFBooleanGetTypeID() { return number.boolValue }
        let real = number.doubleValue
        return real.isFinite && real == real.rounded() && real.magnitude < 9_007_199_254_740_992
            ? Int(real) : real
    }
    if let values = value as? [Any] { return values.map(tachyonSwiftJSONValue) }
    if let values = value as? [String: Any] { return values.mapValues(tachyonSwiftJSONValue) }
    return value
}
