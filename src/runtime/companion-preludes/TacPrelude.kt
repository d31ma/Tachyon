// Tachyon editor prelude for Kotlin Tac companions.
//
// Never compiled or bundled by Tachyon (only `tac.kt` beside a `tac.html` is a
// companion). Copy this file next to your companion so editors and `kotlinc`
// resolve the implicit prelude. A Kotlin companion that compiles with this
// prelude under real `kotlinc` is valid Tac Kotlin — the CI parse check in
// tests/compiler/companion-real-language.test.js relies on that property.

annotation class publish(val name: String = "")
annotation class subscribe(val name: String = "")
annotation class onMount

open class Tac {
    fun publish(name: String, value: Any? = null): Boolean = false
    fun env(key: String, fallback: Any? = null): Any? = fallback
    fun rerender() {}
}

object localStorage {
    fun getItem(key: String, fallback: String = ""): String = fallback
    fun setItem(key: String, value: Any?) {}
    fun removeItem(key: String) {}
}

object sessionStorage {
    fun getItem(key: String, fallback: String = ""): String = fallback
    fun setItem(key: String, value: Any?) {}
    fun removeItem(key: String) {}
}

object navigator {
    fun language(): String = ""
    fun isOnline(): Boolean = false
}

object location {
    fun href(): String = ""
    fun origin(): String = ""
}

fun fetch(url: String, init: Any? = null): Any? = null

class FyloCollection {
    fun find(query: Any?): Any? = null
    fun get(id: Any?): Any? = null
    fun create(document: Any? = null): Any? = null
    fun put(document: Any?): Any? = null
    fun patch(id: Any?, patch: Any? = null): Any? = null
    fun delete(id: Any?): Any? = null
    fun list(): Any? = null
}

object fylo {
    fun collection(name: String): FyloCollection = FyloCollection()
}

class AppInfo {
    val name: String = ""
    val version: String = ""
}

object app {
    fun isAvailable(): Boolean = false
    fun info(): AppInfo = AppInfo()
}

object clipboard {
    fun readText(): String = ""
    fun writeText(text: String) {}
}

object fileSystem {
    fun readText(path: String): String = ""
    fun writeText(path: String, text: String) {}
    fun readDir(path: String): Any? = null
    fun stat(path: String): Any? = null
    fun mkdir(path: String): Any? = null
    fun remove(path: String): Any? = null
    fun paths(): Any? = null
}

object shell {
    fun exec(command: String, args: Any? = null, cwd: String? = null): Any? = null
}

object browser {
    fun open(url: String) {}
}

object share {
    fun text(text: String, title: String? = null) {}
}

object haptics {
    fun impact() {}
}

object filePicker {
    fun openText(): String = ""
    fun saveText(name: String, text: String) {}
}

object capabilities {
    fun supports(capability: String): Boolean = false
    fun state(capability: String): String = "unsupported"
}

object secrets { fun get(key: String): String? = null; fun set(key: String, value: String) {}; fun delete(key: String) {} }
object auth { fun verifyUser(reason: String): Any? = null }
object geolocation { fun current(options: Any? = null): Any? = null }
object notifications { fun show(title: String, options: Any? = null) {} }
object media { fun getUserMedia(constraints: Any?): Any? = null }
