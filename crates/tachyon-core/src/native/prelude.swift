// Appended to a tac.swift companion compiled into an Apple host.
//
// The host and companion share apple_json.swift: one bounded Foundation JSON
// representation, including duplicate-key rejection, Unicode and null values.
// This is trusted application code compiled into the host, not a plugin sandbox.

import Foundation

// MARK: - Persisted fields
//
// The browser gives a page two lifetimes: `$$name` survives every visit, and
// `$name` lasts the tab. A native app has no tabs, so a session is this
// process, and what survives every visit is the platform's own settings store
// — UserDefaults on Apple, which is what `@AppStorage` writes to.
//
// `@Stored` and `@Session` are the same two lifetimes, spelled the way Swift
// spells a property that persists itself.

@propertyWrapper
public struct Stored<Value> {
    private let key: String
    private let fallback: Value

    public init(wrappedValue: Value, _ key: String) {
        self.key = "tachyon." + key
        self.fallback = wrappedValue
    }

    public var wrappedValue: Value {
        get { UserDefaults.standard.object(forKey: key) as? Value ?? fallback }
        nonmutating set { UserDefaults.standard.set(newValue, forKey: key) }
    }
}

/// Session storage: this process, and no longer.
public enum TacSession {
    nonisolated(unsafe) private static var values: [String: Any] = [:]

    public static func get<Value>(_ key: String, _ fallback: Value) -> Value {
        values[key] as? Value ?? fallback
    }

    public static func set(_ key: String, _ value: Any) {
        values[key] = value
    }
}

@propertyWrapper
public struct Session<Value> {
    private let key: String
    private let fallback: Value

    public init(wrappedValue: Value, _ key: String) {
        self.key = key
        self.fallback = wrappedValue
    }

    public var wrappedValue: Value {
        get { TacSession.get(key, fallback) }
        nonmutating set { TacSession.set(key, newValue) }
    }
}

/// One member the island may reach, declared in `tac`.
enum TacMember {
    case field(() -> Any?, ((Any?) -> Void)?)
    case method(([Any?]) -> Any?)

    /// A field the island may read but not write.
    static func field(_ read: @escaping () -> Any?) -> TacMember { .field(read, nil) }
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
    case is Void: return "null"
    case let value as Bool: return value ? "true" : "false"
    case let value as Int: return String(value)
    case let value as Double:
        // `Int(_:)` traps on a value it cannot hold, so infinity, NaN and
        // anything past the range JSON numbers are exact in are ruled out
        // before the conversion rather than after it. The bound is the one
        // `swiftValue` reads back with, so a round trip keeps its type.
        guard value.isFinite else { return "null" }
        return value == value.rounded() && value.magnitude < 9_007_199_254_740_992
            ? String(Int(value)) : String(value)
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
    guard let request = tachyonParseJSONRequest(raw) else {
        return "{\"error\":\"Malformed companion request.\"}"
    }
    guard let operation = request["op"] as? String else {
        return "{\"error\":\"Companion request has no operation.\"}"
    }
    guard let route = request["route"] as? String, let tac = tacRouteMembers(route) else {
        if operation == "init" { return "{\"value\":{\"fields\":[],\"methods\":[]}}" }
        return "{\"error\":\"Unknown companion route.\",\"code\":\"TY_NATIVE_ROUTE\"}"
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
    guard let name = request["name"] as? String, let member = tac[name] else {
        return "{\"error\":\"Unknown companion member.\"}"
    }
    switch (operation, member) {
    case ("get", .field(let read, _)):
        return "{\"value\":" + tacEncode(read()) + "}"
    case ("set", .field(_, let write)):
        guard let write else {
            return "{\"error\":" + tacEncode("Companion field is read-only: " + name) + "}"
        }
        write(tachyonSwiftJSONValue(request["value"]))
        return "{\"value\":null}"
    case ("call", .method(let invoke)):
        let arguments: [Any?]
        if let values = request["args"] as? [Any] {
            arguments = values.map(tachyonSwiftJSONValue)
        } else {
            arguments = []
        }
        return "{\"value\":" + tacEncode(invoke(arguments)) + "}"
    default:
        return "{\"error\":" + tacEncode("Companion member does not support " + operation + ": " + name) + "}"
    }
}


/// Answers one companion request from the host's JavaScript bridge.
///
/// Named rather than `@_cdecl`, because the caller is Swift in the same
/// binary. A native companion is the developer's own compiled code and runs
/// in-process with the host: nothing here bounds what it may do.
func tacNativeInvoke(_ request: String) -> String {
    tacHandle(request)
}

// MARK: - Publishing to the page
//
// Everything above is the page asking a question. This is the other direction,
// and the reason it exists: a companion watching something the platform tells
// it about — a power state, a network change, a file — has no question to
// answer, because nobody asked one.
//
// The sink is installed by the host rather than reached for, so a companion
// compiled without a web view around it publishes into nothing instead of
// failing to link.

public enum TacBridge {
    /// Set once by the host, on the thread its web view lives on.
    nonisolated(unsafe) public static var emit: ((String) -> Void)?
}

/// Publishes a value to the page, where `@subscribe(name)` receives it.
///
/// Safe to call from any thread: the host is what hops to its main thread,
/// because it is the one that knows which thread that is.
public func tacPublish(_ name: String, _ value: Any? = nil) {
    TacBridge.emit?("{\"name\":" + tacEncode(name) + ",\"value\":" + tacEncode(value) + "}")
}
