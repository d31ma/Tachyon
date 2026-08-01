var count = 6
let label = "from Swift"

func doubled() -> Int { count * 2 }

let tac: [String: TacMember] = [
    "count": .field({ count }, { count = $0 as? Int ?? count }),
    "label": .field({ label }),
    "doubled": .method({ _ in doubled() }),
]
