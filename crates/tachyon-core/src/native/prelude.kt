// Appended to a tac.kt companion compiled into an Android host.
//
// The parser is `org.json`, which Android ships, so a companion adds no
// dependency to the project — the same stance the generated host takes.

/** One readable, optionally writable member of a companion. */

// The browser's two lifetimes, on Android. `stored` writes through to the
// platform's own settings store; `session` lasts this process, because a
// native app has no tabs for a tab-scoped value to belong to.
//
// SharedPreferences rather than DataStore: a companion is a plain class with
// no coroutine scope to suspend in, and this is exactly the "small collection
// of key-values" SharedPreferences is for.
private val tacSessionValues = HashMap<String, Any?>()

object TacStore {
    /** Set by the host before the companion is first reached. */
    @JvmStatic
    var preferences: android.content.SharedPreferences? = null

    @JvmStatic
    fun local(key: String, fallback: Any?): Any? {
        val store = preferences ?: return fallback
        return store.all["tachyon.$key"] ?: fallback
    }

    @JvmStatic
    fun setLocal(key: String, value: Any?) {
        val editor = preferences?.edit() ?: return
        when (value) {
            is Int -> editor.putInt("tachyon.$key", value)
            is Long -> editor.putLong("tachyon.$key", value)
            is Boolean -> editor.putBoolean("tachyon.$key", value)
            is Float -> editor.putFloat("tachyon.$key", value)
            else -> editor.putString("tachyon.$key", value?.toString())
        }
        editor.apply()
    }

    @JvmStatic
    fun session(key: String, fallback: Any?): Any? = tacSessionValues[key] ?: fallback

    @JvmStatic
    fun setSession(key: String, value: Any?) {
        tacSessionValues[key] = value
    }
}

class TacField(val read: () -> Any?, val write: ((Any?) -> Unit)? = null)

/** One callable member of a companion. */
class TacMethod(val invoke: (List<Any?>) -> Any?)

// Publishing to the page.
//
// Everything else here is the page asking a question. This is the other
// direction, and the reason it exists: a companion watching something the
// platform tells it about — a battery level, a connectivity change, a sensor —
// has no question to answer, because nobody asked one.
//
// A SAM interface rather than a Kotlin function type, because the host that
// installs it is the generated Java activity.
fun interface TacEmit {
    fun send(payload: String)
}

object TacBridge {
    /** Installed by the host before the page can reach the companion. */
    @JvmStatic
    var emit: TacEmit? = null
}

/**
 * Publishes a value to the page, where `@subscribe(name)` receives it.
 *
 * Safe to call from any thread: the host is what posts to the UI thread,
 * because it is the one holding the WebView.
 */
fun tacPublish(name: String, value: Any? = null) {
    val sink = TacBridge.emit ?: return
    sink.send("{\"name\":${org.json.JSONObject.quote(name)},\"value\":${tacEncode(value)}}")
}

/** Reads an argument or an assigned value as the type the author expects. */
fun tacInt(value: Any?): Int =
    when (value) {
        is Number -> value.toInt()
        is Boolean -> if (value) 1 else 0
        else -> value?.toString()?.toDoubleOrNull()?.toInt() ?: 0
    }

fun tacDouble(value: Any?): Double =
    when (value) {
        is Number -> value.toDouble()
        is Boolean -> if (value) 1.0 else 0.0
        else -> value?.toString()?.toDoubleOrNull() ?: 0.0
    }

fun tacBoolean(value: Any?): Boolean =
    when (value) {
        is Boolean -> value
        is Number -> value.toDouble() != 0.0
        is String -> value.isNotEmpty()
        else -> false
    }

fun tacString(value: Any?): String = value?.toString() ?: ""

private fun tacEncode(value: Any?): String =
    when (value) {
        null, is Unit -> "null"
        is Boolean -> value.toString()
        is Int, is Long, is Short, is Byte -> value.toString()
        is Double -> if (!value.isFinite()) "null" else if (value == Math.floor(value))
            value.toLong().toString() else value.toString()
        is Float -> tacEncode(value.toDouble())
        is String -> org.json.JSONObject.quote(value)
        is Iterable<*> -> value.joinToString(",", "[", "]") { tacEncode(it) }
        // Anything else crosses as its text, which is what a companion
        // returning an unforeseen type would want to see on screen.
        else -> org.json.JSONObject.quote(value.toString())
    }

/** The JSON value an author's closure expects, from one parsed node. */
private fun tacValue(value: Any?): Any? =
    when (value) {
        null, org.json.JSONObject.NULL -> null
        is org.json.JSONArray -> (0 until value.length()).map { tacValue(value.opt(it)) }
        is org.json.JSONObject -> value.toString()
        else -> value
    }

/**
 * Answers one route-scoped companion request (ADR 0019).
 *
 * The host calls this directly: the companion is compiled into the same APK,
 * so there is no module to instantiate and no memory to hand across.
 */
private fun tacBoundedJSON(raw: String): Boolean {
    if (raw.length > 65536 || raw.toByteArray(Charsets.UTF_8).size > 65536) return false
    var depth = 0
    var quoted = false
    var escaped = false
    for (character in raw) {
        if (quoted) {
            if (escaped) escaped = false
            else if (character == '\\') escaped = true
            else if (character == '"') quoted = false
        } else when (character) {
            '"' -> quoted = true
            '{', '[' -> { depth++; if (depth > 64) return false }
            '}', ']' -> { depth--; if (depth < 0) return false }
        }
    }
    return depth == 0 && !quoted
}

fun tacNativeInvoke(request: String): String =
    try {
        require(tacBoundedJSON(request)) { "Invalid or oversized companion request." }
        val parsed = org.json.JSONObject(request)
        val tac = tacRouteMembers(parsed.optString("route"))
            ?: if (parsed.optString("op") == "init") emptyMap<String, Any>()
               else throw IllegalArgumentException("Unknown companion route.")
        when (val operation = parsed.optString("op")) {
            "init" -> {
                val fields = tac.entries.filter { it.value is TacField }.map { it.key }
                val methods = tac.entries.filter { it.value is TacMethod }.map { it.key }
                "{\"value\":{\"fields\":${tacEncode(fields)},\"methods\":${tacEncode(methods)}}}"
            }
            else -> {
                val name = parsed.optString("name")
                when (val member = tac[name]) {
                    null -> "{\"error\":${org.json.JSONObject.quote(
                        "Unknown companion member: $name")}}"
                    is TacField -> when {
                        operation == "get" -> "{\"value\":${tacEncode(member.read())}}"
                        operation == "set" && member.write != null -> {
                            member.write?.invoke(tacValue(parsed.opt("value")))
                            "{\"value\":null}"
                        }
                        operation == "set" -> "{\"error\":${org.json.JSONObject.quote(
                            "Companion field is read-only: $name")}}"
                        else -> "{\"error\":${org.json.JSONObject.quote(
                            "Companion member does not support $operation: $name")}}"
                    }
                    is TacMethod -> if (operation == "call") {
                        val given = parsed.optJSONArray("args")
                        val arguments =
                            (0 until (given?.length() ?: 0)).map { tacValue(given?.opt(it)) }
                        "{\"value\":${tacEncode(member.invoke(arguments))}}"
                    } else "{\"error\":${org.json.JSONObject.quote(
                        "Companion member does not support $operation: $name")}}"
                    else -> "{\"error\":\"Unknown companion member kind.\"}"
                }
            }
        }
    } catch (error: Throwable) {
        "{\"error\":${org.json.JSONObject.quote(error.message ?: "Companion failed.")}}"
    }
