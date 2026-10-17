@_extern(wasm, module: "tachyon", name: "tac_host_http_start")
func tacHostHttpStart(_ ticket: Int32, _ pointer: UnsafePointer<UInt8>?, _ length: Int32)

@_extern(wasm, module: "tachyon", name: "tac_host_http_cancel")
func tacHostHttpCancel(_ ticket: Int32)

enum TacHttpTickets {
    static let outsideTask = "{\"error\":{\"code\":\"HTTP_REQUEST\"}}"
    static let cancelled = "{\"error\":{\"code\":\"CANCELLED\"}}"
    nonisolated(unsafe) static var last = 0
    nonisolated(unsafe) static var waiting: [Int: (TacRunningTask, CheckedContinuation<String, Never>)] = [:]

    static func exchange(_ request: String) async -> String {
        await started(request) { ticket, pointer, length in tacHostHttpStart(ticket, pointer, length) }
    }

    static func started(
        _ request: String, _ start: (Int32, UnsafePointer<UInt8>?, Int32) -> Void
    ) async -> String {
        guard let running = TacTasks.running else { return outsideTask }
        guard !Task.isCancelled else { return cancelled }
        return await withCheckedContinuation { continuation in
            last += 1
            let ticket = last
            waiting[ticket] = (running, continuation)
            var text = request
            text.withUTF8 { start(Int32(ticket), $0.baseAddress, Int32($0.count)) }
        }
    }

    static func settle(_ ticket: Int, _ answer: String) {
        guard let (running, continuation) = waiting.removeValue(forKey: ticket) else { return }
        TacTasks.resume(running) { continuation.resume(returning: answer) }
    }

    static func stop(_ running: TacRunningTask) {
        let tickets = waiting.filter { $0.value.0 === running }.keys.sorted()
        for ticket in tickets {
            guard let (_, continuation) = waiting.removeValue(forKey: ticket) else { continue }
            tacHostHttpCancel(Int32(ticket))
            continuation.resume(returning: cancelled)
        }
    }
}

@_expose(wasm, "tac_http_settle")
@_cdecl("tac_http_settle")
func tacHttpSettle(_ ticket: Int32, _ pointer: UnsafePointer<UInt8>, _ length: Int32) {
    TacHttpTickets.settle(Int(ticket), String(decoding: UnsafeBufferPointer(start: pointer, count: Int(length)), as: UTF8.self))
}
