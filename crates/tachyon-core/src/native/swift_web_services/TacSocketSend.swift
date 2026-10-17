@_extern(wasm, module: "tachyon", name: "tac_host_socket_send")
func tacHostSocketSend(_ ticket: Int32, _ pointer: UnsafePointer<UInt8>?, _ length: Int32)

struct TacSocketError: Error, CustomStringConvertible {
    static let known: Set<String> = ["HTTP_BODY", "HTTP_MISSING", "HTTP_REQUEST", "CANCELLED"]
    let code: String

    init(_ code: String?) {
        if let code, TacSocketError.known.contains(code) {
            self.code = code
        } else {
            self.code = "HTTP_NETWORK"
        }
    }

    var description: String { code + ": The socket message was not sent." }
}

func tacSocketSend<Message: Encodable>(_ client: String, _ message: Message) async throws {
    let text = tacEncode(tacModelValue(message))
    guard text.utf8.count <= 65536 else { throw TacSocketError("HTTP_BODY") }
    let owner = TacTasks.running?.owner ?? ""
    let request = "{\"owner\":" + tacEncode(owner) + ",\"client\":" + tacEncode(client) + ",\"message\":" + text + "}"
    let answer = await TacHttpTickets.started(request) { ticket, pointer, length in tacHostSocketSend(ticket, pointer, length) }
    guard let reply = tacParseJSON(answer) as? [String: Any?] else { throw TacSocketError(nil) }
    if let failure = (reply["error"] ?? nil) as? [String: Any?] { throw TacSocketError((failure["code"] ?? nil) as? String) }
    guard reply.keys.contains("value") else { throw TacSocketError(nil) }
}
