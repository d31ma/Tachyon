// Appended to a tac.kt companion. The author writes plain Kotlin and declares
// which members the island may reach; everything below is the ABI of ADR 0011.
//
// Kotlin/Wasm has no reflection, so the members cannot be discovered: they are
// declared once, in `tac`, as closures over the author's own properties and
// functions. Values cross as JavaScript values rather than as JSON text,
// because the module the compiler emits beside this one is JavaScript anyway
// and is the only thing that can reach the browser.

/** One readable, optionally writable member of a companion. */
class TacField(val read: () -> Any?, val write: ((JsAny?) -> Unit)? = null)

/** One callable member of a companion. */
class TacMethod(val invoke: (List<JsAny?>) -> Any?)

/** Reads an argument or an assigned value as the type the author expects. */
fun tacInt(value: JsAny?): Int = (value as JsNumber).toDouble().toInt()

fun tacDouble(value: JsAny?): Double = (value as JsNumber).toDouble()

fun tacBoolean(value: JsAny?): Boolean = (value as JsBoolean).toBoolean()

fun tacString(value: JsAny?): String = (value as JsString).toString()

private fun tacToJs(value: Any?): JsAny? =
    when (value) {
        null -> null
        is Int -> value.toJsNumber()
        is Long -> value.toDouble().toJsNumber()
        is Double -> value.toJsNumber()
        is Float -> value.toDouble().toJsNumber()
        is Boolean -> value.toJsBoolean()
        is String -> value.toJsString()
        // Scalars and null cross as themselves, anything else as its
        // toString: a structured value needs a JsAny conversion per shape, and
        // there is one to write when a companion actually returns one.
        else -> value.toString().toJsString()
    }

private fun tacField(name: String): TacField =
    tac[name] as? TacField ?: throw IllegalArgumentException("Unknown companion field: $name")

private fun tacMethod(name: String): TacMethod =
    tac[name] as? TacMethod ?: throw IllegalArgumentException("Unknown companion method: $name")

@JsExport
fun tacFields(): String =
    tac.entries.filter { it.value is TacField }.joinToString(",") { it.key }

@JsExport
fun tacMethods(): String =
    tac.entries.filter { it.value is TacMethod }.joinToString(",") { it.key }

@JsExport
fun tacGet(name: String): JsAny? = tacToJs(tacField(name).read())

@JsExport
fun tacSet(name: String, value: JsAny?) {
    val write =
        tacField(name).write ?: throw IllegalArgumentException("Companion field is read-only: $name")
    write(value)
}

@JsExport
fun tacCall(name: String, args: JsArray<JsAny?>?): JsAny? {
    val arguments = buildList { for (index in 0 until (args?.length ?: 0)) add(args?.get(index)) }
    return tacToJs(tacMethod(name).invoke(arguments))
}
