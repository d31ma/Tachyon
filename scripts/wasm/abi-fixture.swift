// A Swift browser companion, written as plain Swift. There is no tac_invoke
// here and no subset of the language: this is compiled by the swift.org
// compiler against the Swift SDK for WebAssembly, and the prelude the build
// appends carries the ABI of ADR 0011.
//
// Members are declared because the host must know a field from a method, and
// because a companion should decide what the page can reach.

var count = 6
let label = "from swift"

func doubled() -> Int { count * 2 }

let tac: [String: TacMember] = [
    "count": .field({ count }, { count = $0 as? Int ?? count }),
    "label": .field({ label }),
    "doubled": .method({ _ in doubled() }),
]
