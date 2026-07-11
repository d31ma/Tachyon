// Tachyon editor prelude for Swift Tac companions.
//
// Never compiled or bundled by Tachyon (only `tac.swift` beside a `tac.html`
// is a companion). Copy this file next to your companion so SourceKit
// resolves the implicit prelude symbols.
//
// ponytail: swiftc still rejects @onMount/@publish on functions (Swift has no
// arbitrary attributes); this stub only fixes symbol resolution. Swift
// companions are a Tac dialect, not real Swift — see the portable companion
// subset reference in the Tachyon README.

open class Tac {
    public init() {}
    public func publish(_ name: String, _ value: Any? = nil) -> Bool { false }
    public func env(_ key: String, _ fallback: Any? = nil) -> Any? { fallback }
    public func rerender() {}
}

public struct StorageApi {
    public func getItem(_ key: String, _ fallback: String = "") -> String { fallback }
    public func setItem(_ key: String, _ value: Any?) {}
    public func removeItem(_ key: String) {}
}
public let localStorage = StorageApi()
public let sessionStorage = StorageApi()

public struct NavigatorApi {
    public func language() -> String { "" }
    public func isOnline() -> Bool { false }
}
public let navigator = NavigatorApi()

public struct LocationApi {
    public func href() -> String { "" }
    public func origin() -> String { "" }
}
public let location = LocationApi()

public func fetch(_ url: String, _ options: Any? = nil) -> Any? { nil }

public struct FyloCollection {
    public func find(_ query: Any?) -> Any? { nil }
    public func get(_ id: Any?) -> Any? { nil }
    public func create(_ document: Any? = nil) -> Any? { nil }
    public func put(_ document: Any?) -> Any? { nil }
    public func delete(_ id: Any?) -> Any? { nil }
}
public struct FyloApi {
    public func collection(_ name: String) -> FyloCollection { FyloCollection() }
}
public let fylo = FyloApi()

public struct AppInfo { public let name = ""; public let version = "" }
public struct AppApi {
    public func isAvailable() -> Bool { false }
    public func info() -> AppInfo { AppInfo() }
}
public let app = AppApi()

public struct ClipboardApi {
    public func readText() -> String { "" }
    public func writeText(_ text: String) {}
}
public let clipboard = ClipboardApi()

public struct FileSystemApi {
    public func readText(_ path: String) -> String { "" }
    public func writeText(_ path: String, _ text: String) {}
    public func readDir(_ path: String) -> Any? { nil }
    public func paths() -> Any? { nil }
}
public let fileSystem = FileSystemApi()

public struct ShellApi {
    public func exec(_ command: String, _ args: [String] = [], _ cwd: String? = nil) -> Any? { nil }
}
public let shell = ShellApi()

public struct BrowserApi {
    public func open(_ url: String) {}
}
public let browser = BrowserApi()

public struct ShareApi {
    public func text(_ text: String, _ title: String? = nil) {}
}
public let share = ShareApi()

public struct HapticsApi {
    public func impact() {}
}
public let haptics = HapticsApi()

public struct FilePickerApi {
    public func openText() -> String { "" }
    public func saveText(_ name: String, _ text: String) {}
}
public let filePicker = FilePickerApi()

public struct CapabilitiesApi {
    public func supports(_ capability: String) -> Bool { false }
    public func state(_ capability: String) -> String { "unsupported" }
}
public let capabilities = CapabilitiesApi()

public struct SecretsApi { public func get(_ key: String) -> String? { nil }; public func set(_ key: String, _ value: String) {}; public func delete(_ key: String) {} }
public let secrets = SecretsApi()
public struct AuthApi { public func verifyUser(_ reason: String) -> Any? { nil } }
public let auth = AuthApi()
public struct GeolocationApi { public func current(_ options: Any? = nil) -> Any? { nil } }
public let geolocation = GeolocationApi()
public struct NotificationsApi { public func show(_ title: String, _ options: Any? = nil) {} }
public let notifications = NotificationsApi()
public struct MediaApi { public func getUserMedia(_ constraints: Any?) -> Any? { nil } }
public let media = MediaApi()
