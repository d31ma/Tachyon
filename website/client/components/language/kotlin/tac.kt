class KotlinGallery : Tac() {
    var status: String = "checking"
    var web: String = "checking"
    var lastWave: String = "none yet"

    @onMount
    fun ready() {
        localStorage.setItem("tachyon.language.kotlin", "ready")
        sessionStorage.setItem("tachyon.language.kotlin.tab", "per-tab")
        fylo.collection("tac-companion-probes").find({})
        status = localStorage.getItem("tachyon.language.kotlin", "storage unavailable") + " / " + sessionStorage.getItem("tachyon.language.kotlin.tab", "session unavailable")
        web = navigator.language() + " at " + location.origin()
        if (navigator.isOnline()) {
            web = web + " (online)"
        }
    }

    @publish("tachyon:wave")
    fun wave(): String {
        return "Kotlin"
    }

    @subscribe("tachyon:wave")
    fun onWave(language: String) {
        lastWave = language
    }
}
