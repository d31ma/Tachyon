import Foundation

struct SwiftFyloRepository {
    private let root = ProcessInfo.processInfo.environment["FYLO_ROOT"] ?? "\(FileManager.default.currentDirectoryPath)/db"
    private let executable = ProcessInfo.processInfo.environment["FYLO_EXEC_PATH"]

    func joinSample(_ requestId: String) -> [String: Any] {
        let left = "language-route-events"
        let right = "language-route-relations"
        _ = machine(["op": "createCollection", "collection": left])
        _ = machine(["op": "createCollection", "collection": right])
        _ = machine(["op": "putData", "collection": left, "data": ["language": "swift", "source": "fylo.exec", "requestId": requestId, "group": "join"]])
        _ = machine(["op": "putData", "collection": right, "data": ["group": "join", "related": "swift", "requestId": requestId]])
        _ = machine(["op": "joinDocs", "join": ["$leftCollection": left, "$rightCollection": right, "$mode": "inner", "$on": ["group": ["$eq": "group"]], "$limit": 5]])
        return ["collection": left, "operations": ["createCollection", "putData", "joinDocs"], "resultCount": "5"]
    }

    private func machine(_ request: [String: Any]) -> Any? {
        let process = Process()
        if let executable, !executable.isEmpty {
            process.executableURL = URL(fileURLWithPath: executable)
            process.arguments = ["exec", "--request", "-", "--root", root]
        } else {
            process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            process.arguments = ["bunx", "--bun", "fylo.exec", "exec", "--request", "-", "--root", root]
        }
        let input = Pipe()
        let output = Pipe()
        let error = Pipe()
        process.standardInput = input
        process.standardOutput = output
        process.standardError = error
        try! process.run()
        input.fileHandleForWriting.write(try! JSONSerialization.data(withJSONObject: request))
        input.fileHandleForWriting.closeFile()
        process.waitUntilExit()
        let stdout = output.fileHandleForReading.readDataToEndOfFile()
        let stderr = String(data: error.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        if process.terminationStatus != 0 { fatalError(stderr.isEmpty ? "fylo.exec failed" : stderr) }
        let response = (try! JSONSerialization.jsonObject(with: stdout.isEmpty ? Data("{}".utf8) : stdout)) as! [String: Any]
        guard response["ok"] as? Bool == true else { fatalError("fylo.exec returned an error") }
        return response["result"]
    }
}
