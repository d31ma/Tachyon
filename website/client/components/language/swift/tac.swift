final class SwiftGallery: Tac {
    var status: String = "checking"
    var fetchStatus: String = "not fetched yet"
    var portable: String = "checking"
    var lastWave: String = "none yet"

    @onMount
    func ready() {
        localStorage.setItem("tachyon.language.swift", "ready")
        fylo.collection("tac-companion-probes").find({})
        self.status = localStorage.getItem("tachyon.language.swift", "storage unavailable")
        self.portable = "device APIs need a native bundle"
        if capabilities.supports("web.fetch") {
            self.portable = "web.fetch is portable everywhere"
        }
        if capabilities.supports("share.text") {
            self.portable = self.portable + ", share.text is native here"
        }
    }

    func probeFetch() {
        let response = await fetch(location.href())
        self.fetchStatus = "HTTP " + response.status + " via local-first fetch()"
    }

    @publish("tachyon:wave")
    func wave() -> String {
        return "Swift"
    }

    @subscribe("tachyon:wave")
    func onWave(language: String) {
        self.lastWave = language
    }
}
