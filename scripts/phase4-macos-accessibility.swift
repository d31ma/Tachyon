import AppKit
import ApplicationServices
import Foundation

private struct ElementSnapshot: Codable {
    let role: String
    let identifier: String?
    let title: String?
    let label: String?
    let value: String?
    let enabled: Bool?
    let focused: Bool?
}

private struct Evidence: Codable {
    let process: String
    let interactions: [String]
    let elements: [ElementSnapshot]
}

private enum ProbeError: Error, CustomStringConvertible {
    case usage
    case accessibilityDisabled
    case processNotFound(String)
    case windowNotFound
    case controlNotFound(String)
    case actionFailed(String, AXError)
    case focusFailed(String, AXError)
    case keyboardFailed

    var description: String {
        switch self {
        case .usage:
            return "usage: phase4-macos-accessibility PROCESS [--interact]"
        case .accessibilityDisabled:
            return "macOS Accessibility permission is not enabled for this process"
        case let .processNotFound(name):
            return "application process '\(name)' was not found"
        case .windowNotFound:
            return "the application has no accessible window"
        case let .controlNotFound(name):
            return "accessible control '\(name)' was not found"
        case let .actionFailed(name, error):
            return "AXPress failed for '\(name)' with \(error.rawValue)"
        case let .focusFailed(name, error):
            return "AXFocused failed for '\(name)' with \(error.rawValue)"
        case .keyboardFailed:
            return "a CoreGraphics keyboard event could not be created"
        }
    }
}

private func attribute(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else {
        return nil
    }
    return value
}

private func stringAttribute(_ element: AXUIElement, _ name: String) -> String? {
    guard let value = attribute(element, name) else { return nil }
    if CFGetTypeID(value) == CFStringGetTypeID() {
        return value as? String
    }
    if CFGetTypeID(value) == CFNumberGetTypeID() {
        return String(describing: value)
    }
    if CFGetTypeID(value) == CFBooleanGetTypeID() {
        return (value as? NSNumber)?.boolValue == true ? "true" : "false"
    }
    return nil
}

private func boolAttribute(_ element: AXUIElement, _ name: String) -> Bool? {
    guard let value = attribute(element, name), CFGetTypeID(value) == CFBooleanGetTypeID() else {
        return nil
    }
    return (value as? NSNumber)?.boolValue
}

private func children(_ element: AXUIElement) -> [AXUIElement] {
    (attribute(element, kAXChildrenAttribute) as? [AXUIElement]) ?? []
}

private func collect(_ root: AXUIElement, depth: Int = 0) -> [AXUIElement] {
    guard depth <= 128 else { return [] }
    return [root] + children(root).flatMap { collect($0, depth: depth + 1) }
}

private func snapshot(_ element: AXUIElement) -> ElementSnapshot? {
    guard let role = stringAttribute(element, kAXRoleAttribute) else { return nil }
    return ElementSnapshot(
        role: role,
        identifier: stringAttribute(element, kAXIdentifierAttribute),
        title: stringAttribute(element, kAXTitleAttribute),
        label: stringAttribute(element, kAXDescriptionAttribute),
        value: stringAttribute(element, kAXValueAttribute),
        enabled: boolAttribute(element, kAXEnabledAttribute),
        focused: boolAttribute(element, kAXFocusedAttribute)
    )
}

private func matches(_ element: AXUIElement, identifier: String, label: String, role: String) -> Bool {
    let candidateRole = stringAttribute(element, kAXRoleAttribute)
    let candidateIdentifier = stringAttribute(element, kAXIdentifierAttribute)
    let candidateLabel = stringAttribute(element, kAXDescriptionAttribute)
    let candidateTitle = stringAttribute(element, kAXTitleAttribute)
    return candidateRole == role
        && (candidateIdentifier == identifier || candidateLabel == label || candidateTitle == label)
}

private func press(_ element: AXUIElement, name: String) throws {
    let result = AXUIElementPerformAction(element, kAXPressAction as CFString)
    guard result == .success else { throw ProbeError.actionFailed(name, result) }
}

private func typeText(_ value: String, into element: AXUIElement, processID: pid_t) throws {
    let focus = AXUIElementSetAttributeValue(
        element,
        kAXFocusedAttribute as CFString,
        kCFBooleanTrue
    )
    guard focus == .success else { throw ProbeError.focusFailed("Customer name", focus) }

    guard let selectAllDown = CGEvent(
        keyboardEventSource: nil,
        virtualKey: 0,
        keyDown: true
    ), let selectAllUp = CGEvent(
        keyboardEventSource: nil,
        virtualKey: 0,
        keyDown: false
    ), let textDown = CGEvent(
        keyboardEventSource: nil,
        virtualKey: 0,
        keyDown: true
    ), let textUp = CGEvent(
        keyboardEventSource: nil,
        virtualKey: 0,
        keyDown: false
    ) else {
        throw ProbeError.keyboardFailed
    }
    selectAllDown.flags = .maskCommand
    selectAllUp.flags = .maskCommand
    selectAllDown.postToPid(processID)
    selectAllUp.postToPid(processID)

    for character in value {
        var characters = Array(String(character).utf16)
        textDown.keyboardSetUnicodeString(stringLength: characters.count, unicodeString: &characters)
        textUp.keyboardSetUnicodeString(stringLength: characters.count, unicodeString: &characters)
        textDown.postToPid(processID)
        textUp.postToPid(processID)
        RunLoop.current.run(until: Date().addingTimeInterval(0.08))
        guard boolAttribute(element, kAXFocusedAttribute) == true else {
            throw ProbeError.focusFailed("Customer name after reactive input", .failure)
        }
    }
}

private func run() throws {
    guard CommandLine.arguments.count >= 2 else { throw ProbeError.usage }
    guard AXIsProcessTrusted() else { throw ProbeError.accessibilityDisabled }
    let processName = CommandLine.arguments[1]
    guard let application = NSWorkspace.shared.runningApplications.first(where: {
        $0.localizedName == processName
    }) else {
        throw ProbeError.processNotFound(processName)
    }
    let app = AXUIElementCreateApplication(application.processIdentifier)
    guard let windows = attribute(app, kAXWindowsAttribute) as? [AXUIElement],
          let window = windows.first else {
        throw ProbeError.windowNotFound
    }

    var interactions: [String] = []
    if CommandLine.arguments.contains("--interact") {
        application.activate()
        var elements = collect(window)
        guard let button = elements.first(where: {
            matches($0, identifier: "n_000006", label: "Increase count", role: kAXButtonRole)
        }) else {
            throw ProbeError.controlNotFound("Increase count")
        }
        try press(button, name: "Increase count")
        interactions.append("increment")
        RunLoop.current.run(until: Date().addingTimeInterval(0.25))
        // A reactive render can replace DOM-backed accessibility objects.
        elements = collect(window)

        guard let textField = elements.first(where: {
            matches($0, identifier: "n_000008", label: "Customer name", role: kAXTextFieldRole)
        }) else {
            throw ProbeError.controlNotFound("Customer name")
        }
        try typeText("Ada", into: textField, processID: application.processIdentifier)
        interactions.append("input")
        RunLoop.current.run(until: Date().addingTimeInterval(0.5))

        elements = collect(window)
        guard let disclosure = elements.first(where: {
            stringAttribute($0, kAXRoleAttribute) == kAXDisclosureTriangleRole
        }) else {
            throw ProbeError.controlNotFound("Implementation details")
        }
        let focus = AXUIElementSetAttributeValue(disclosure, kAXFocusedAttribute as CFString, kCFBooleanTrue)
        guard focus == .success else { throw ProbeError.focusFailed("Implementation details", focus) }
        guard let down = CGEvent(keyboardEventSource: nil, virtualKey: 49, keyDown: true),
              let up = CGEvent(keyboardEventSource: nil, virtualKey: 49, keyDown: false) else {
            throw ProbeError.keyboardFailed
        }
        down.postToPid(application.processIdentifier)
        up.postToPid(application.processIdentifier)
        interactions.append("disclosure")
        RunLoop.current.run(until: Date().addingTimeInterval(0.35))
    }

    let evidence = Evidence(
        process: processName,
        interactions: interactions,
        elements: collect(window).compactMap(snapshot)
    )
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
    FileHandle.standardOutput.write(try encoder.encode(evidence))
    FileHandle.standardOutput.write(Data([0x0A]))
}

do {
    try run()
} catch {
    FileHandle.standardError.write(Data("phase4 accessibility probe: \(error)\n".utf8))
    exit(1)
}
