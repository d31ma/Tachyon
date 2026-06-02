import Foundation

struct SwiftLanguageService {
    private let fyloRepository = SwiftFyloRepository()

    func describe(_ request: [String: Any]) -> [String: Any] {
        var requestId = "unknown"
        if let context = request["context"] as? [String: Any],
           let value = context["requestId"] as? String {
            requestId = value
        }

        return [
            "language": "swift",
            "message": "Hello from Swift!",
            "requestId": requestId,
            "fylo": fyloRepository.joinSample(requestId),
        ]
    }
}
