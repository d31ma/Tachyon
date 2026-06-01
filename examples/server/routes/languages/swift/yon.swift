import Foundation

enum Handler {
    static func GET(_ request: [String: Any]) -> Any? {
        switch statusCode(request) {
        case "507": return ["code": "507", "detail": "insufficient storage"]
        case "508": return ["code": "508", "detail": "loop detected"]
        case "510": return ["code": "510", "detail": "not extended"]
        case "511": return ["code": "511", "detail": "network authentication required"]
        default: return SwiftLanguageService().describe(request)
        }
    }

    private static func statusCode(_ request: [String: Any]) -> String {
        guard let query = request["query"] as? [String: Any] else { return "" }
        if let code = query["code"] as? String { return code }
        if let code = query["code"] as? Int { return String(code) }
        return ""
    }
}
