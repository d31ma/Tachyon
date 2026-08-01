// Appended to a tac.swift companion. The author writes plain Swift and
// declares which members the island may reach; everything below is the ABI of
// ADR 0011 — memory, tac_alloc, tac_invoke.
//
// The JSON is scanned here rather than by Foundation: the same fixture is
// 5.5 MB with the standard library alone and 53 MB once Foundation is linked,
// and a browser companion cannot carry that. The protocol's shape is fixed and
// small, so scanning it is a page of code rather than a dependency.

/// One member the island may reach, declared in `tac`.
enum TacMember {
    case field(() -> Any?, ((Any?) -> Void)?)
    case method(([Any?]) -> Any?)

    /// A field the island may read but not write.
    static func field(_ read: @escaping () -> Any?) -> TacMember { .field(read, nil) }
}

private enum TacJSON {
    case null
    case flag(Bool)
    case number(Double)
    case text(String)
    case list([TacJSON])
    case object([String: TacJSON])

    /// The Swift value an author's closure expects. A whole number arrives as
    /// `Int`, because JSON has one number type and a companion counting things
    /// wrote `as? Int`.
    var swiftValue: Any? {
        switch self {
        case .null: return nil
        case .flag(let value): return value
        case .number(let value):
            return value == value.rounded() && value.magnitude < 9_007_199_254_740_992
                ? Int(value) : value
        case .text(let value): return value
        case .list(let values): return values.map(\.swiftValue)
        case .object(let values): return values.mapValues(\.swiftValue)
        }
    }
}

private struct TacScanner {
    let characters: [Character]
    var index = 0

    init(_ raw: String) { characters = Array(raw) }

    mutating func skipWhitespace() {
        while index < characters.count, characters[index] == " " || characters[index] == "\n"
            || characters[index] == "\t" || characters[index] == "\r"
        {
            index += 1
        }
    }

    mutating func value() -> TacJSON {
        skipWhitespace()
        guard index < characters.count else { return .null }
        switch characters[index] {
        case "{": return object()
        case "[": return list()
        case "\"": return .text(string())
        case "t": index += 4; return .flag(true)
        case "f": index += 5; return .flag(false)
        case "n": index += 4; return .null
        default: return .number(number())
        }
    }

    mutating func object() -> TacJSON {
        var entries: [String: TacJSON] = [:]
        index += 1
        while index < characters.count {
            skipWhitespace()
            if characters[index] == "}" { index += 1; break }
            if characters[index] == "," { index += 1; continue }
            let key = string()
            skipWhitespace()
            if index < characters.count, characters[index] == ":" { index += 1 }
            entries[key] = value()
        }
        return .object(entries)
    }

    mutating func list() -> TacJSON {
        var values: [TacJSON] = []
        index += 1
        while index < characters.count {
            skipWhitespace()
            if characters[index] == "]" { index += 1; break }
            if characters[index] == "," { index += 1; continue }
            values.append(value())
        }
        return .list(values)
    }

    mutating func string() -> String {
        var text = ""
        guard index < characters.count, characters[index] == "\"" else { return text }
        index += 1
        while index < characters.count {
            let character = characters[index]
            index += 1
            if character == "\"" { break }
            guard character == "\\", index < characters.count else {
                text.append(character)
                continue
            }
            let escaped = characters[index]
            index += 1
            switch escaped {
            case "n": text.append("\n")
            case "t": text.append("\t")
            case "r": text.append("\r")
            case "b": text.append("\u{8}")
            case "f": text.append("\u{c}")
            case "u":
                let digits = String(characters[index..<min(index + 4, characters.count)])
                index += 4
                if let scalar = UInt32(digits, radix: 16).flatMap(Unicode.Scalar.init) {
                    text.append(Character(scalar))
                }
            default: text.append(escaped)
            }
        }
        return text
    }

    mutating func number() -> Double {
        var digits = ""
        while index < characters.count, "0123456789+-.eE".contains(characters[index]) {
            digits.append(characters[index])
            index += 1
        }
        return Double(digits) ?? 0
    }
}

private func tacEncode(_ text: String) -> String {
    var encoded = "\""
    for character in text.unicodeScalars {
        switch character {
        case "\"": encoded += "\\\""
        case "\\": encoded += "\\\\"
        case "\n": encoded += "\\n"
        case "\t": encoded += "\\t"
        case "\r": encoded += "\\r"
        default:
            if character.value < 0x20 {
                var hexadecimal = String(character.value, radix: 16)
                while hexadecimal.count < 4 { hexadecimal = "0" + hexadecimal }
                encoded += "\\u" + hexadecimal
            } else {
                encoded.unicodeScalars.append(character)
            }
        }
    }
    return encoded + "\""
}

private func tacEncode(_ value: Any?) -> String {
    switch value {
    case nil: return "null"
    case let value as Bool: return value ? "true" : "false"
    case let value as Int: return String(value)
    case let value as Double:
        return value == value.rounded() ? String(Int(value)) : String(value)
    case let value as String: return tacEncode(value)
    case let values as [Any?]: return "[" + values.map(tacEncode).joined(separator: ",") + "]"
    case let values as [String: Any?]:
        return "{"
            + values.map { tacEncode($0.key) + ":" + tacEncode($0.value) }.joined(separator: ",")
            + "}"
    // Anything else crosses as its description, which is what a companion
    // returning an unforeseen type would want to see in the page.
    case let value: return tacEncode(String(describing: value!))
    }
}

private func tacHandle(_ raw: String) -> String {
    var scanner = TacScanner(raw)
    guard case .object(let request) = scanner.value() else {
        return "{\"error\":\"Companion request is not an object.\"}"
    }
    guard case .text(let operation)? = request["op"] else {
        return "{\"error\":\"Companion request has no operation.\"}"
    }
    if operation == "init" {
        var fields: [Any?] = []
        var methods: [Any?] = []
        for (name, member) in tac {
            if case .field = member { fields.append(name) } else { methods.append(name) }
        }
        return "{\"value\":{\"fields\":" + tacEncode(fields) + ",\"methods\":"
            + tacEncode(methods) + "}}"
    }
    guard case .text(let name)? = request["name"], let member = tac[name] else {
        return "{\"error\":\"Unknown companion member.\"}"
    }
    switch (operation, member) {
    case ("get", .field(let read, _)):
        return "{\"value\":" + tacEncode(read()) + "}"
    case ("set", .field(_, let write)):
        guard let write else {
            return "{\"error\":\"Companion field is read-only: " + name + "\"}"
        }
        write((request["value"] ?? .null).swiftValue)
        return "{\"value\":null}"
    case ("call", .method(let invoke)):
        let arguments: [Any?]
        if case .list(let values)? = request["args"] {
            arguments = values.map(\.swiftValue)
        } else {
            arguments = []
        }
        return "{\"value\":" + tacEncode(invoke(arguments)) + "}"
    default:
        return "{\"error\":\"Companion member does not support " + operation + ": " + name + "\"}"
    }
}

// A companion allocates a handful of small buffers per interaction and the
// module is discarded with the page, so nothing is freed.
@_cdecl("tac_alloc")
func tacAlloc(_ size: Int32) -> Int32 {
    let raw = UnsafeMutableRawPointer.allocate(byteCount: max(Int(size), 1), alignment: 1)
    return Int32(bitPattern: UInt32(UInt(bitPattern: raw)))
}

@_cdecl("tac_invoke")
func tacInvoke(_ pointer: Int32, _ length: Int32) -> Int64 {
    guard let start = UnsafeRawPointer(bitPattern: UInt(UInt32(bitPattern: pointer))) else {
        return 0
    }
    let request = String(
        decoding: UnsafeRawBufferPointer(start: start, count: Int(length)), as: UTF8.self)
    let response = Array(tacHandle(request).utf8)
    let target = tacAlloc(Int32(response.count))
    guard let destination = UnsafeMutableRawPointer(bitPattern: UInt(UInt32(bitPattern: target)))
    else { return 0 }
    response.withUnsafeBytes { destination.copyMemory(from: $0.baseAddress!, byteCount: $0.count) }
    return (Int64(target) << 32) | Int64(response.count)
}
