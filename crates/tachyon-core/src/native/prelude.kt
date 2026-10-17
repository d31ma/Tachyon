class TacField(val read: () -> Any?, val write: ((Any?) -> Unit)? = null)

class TacMethod(val invoke: (List<Any?>) -> Any?)

class TacTask(val code: (Throwable) -> String?, val invoke: suspend (List<Any?>) -> Any?)

private fun tacThrownCode(error: Throwable): String? = null

class TacListener(
    val events: List<String>,
    val replay: Boolean,
    val order: Int,
    condition: String?,
    models: String?,
    val returns: String?,
    val asynchronous: Boolean,
    val invoke: (Any?) -> Any?,
) {
    val condition: Any? = tacParse(condition)
    val models: Any? = tacParse(models)
}
class TacHook(val destroy: Boolean, val invoke: () -> Unit)

class TacStream(val client: String, val path: String, val event: String, val socket: Boolean, val models: String?)

class ApplicationEventPublisher {
    fun <T : Any> publishEvent(value: T, retain: Boolean = false) {
        val plain = tacPlain(value)
        if (plain !is Map<*, *>) {
            tacEventFailure()
            return
        }
        tacEventPublish(value::class.simpleName ?: "", plain, retain)
    }
}

private fun tacParse(text: String?): Any? {
    if (text == null) return null
    return try { tacParseJson(text) } catch (_: Exception) { null }
}

private fun tacPlain(value: Any?): Any? =
    when (value) {
        null, is Unit -> null
        is String, is Boolean, is Number -> value
        is Enum<*> -> value.name
        is Map<*, *> -> value.entries.associate { it.key.toString() to tacPlain(it.value) }
        is Iterable<*> -> value.map { tacPlain(it) }
        is Array<*> -> value.map { tacPlain(it) }
        else -> tacPlainModel(value)
    }

private fun tacFits(models: Any?, shape: String, value: Any?): Boolean {
    if (shape.startsWith("?")) return value == null || tacFits(models, shape.substring(1), value)
    if (shape.startsWith("[]")) return value is List<*> && value.all { tacFits(models, shape.substring(2), it) }
    when (shape) {
        "any" -> return true
        "string" -> return value is String
        "integer" -> return value is Int || value is Long || tacIsBigWhole(value)
        "number" -> return value is Number
        "boolean" -> return value is Boolean
    }
    val model = (models as? Map<*, *>)?.get(shape) as? Map<*, *> ?: return false
    val fields = model["fields"] as? Map<*, *> ?: return false
    val objectValue = value as? Map<*, *> ?: return false
    if (model["open"] != true && objectValue.keys.any { !fields.containsKey(it) }) return false
    for ((name, declared) in fields) {
        val field = declared as? String ?: return false
        val required = field.startsWith("!")
        if (!objectValue.containsKey(name)) {
            if (required) return false
            continue
        }
        if (!tacFits(models, if (required) field.substring(1) else field, objectValue[name])) return false
    }
    return true
}

private class TacActivation(val route: String, val service: String, val owner: String, val members: Map<String, Any>) {
    var disposed = false
    val queue = ArrayDeque<Triple<String, String, Boolean>>()
    val retained = linkedMapOf<String, String>()
    val deferred = ArrayDeque<Pair<TacListener, String>>()
    val sorted: List<Any> = members.entries.sortedBy { it.key }.map { it.value }
    val listeners: List<TacListener> = sorted.filterIsInstance<TacListener>().sortedBy { it.order }
}
private object TacNativeState {
    const val START_UP_REQUEST_LIMIT = 1048576
    const val MEMBER_REQUEST_LIMIT = 65536
    val activations = linkedMapOf<String, TacActivation>()
    var executing: TacActivation? = null
    var handling = false
}
private class TacServiceNode(val identity: String, val page: TacPageScope, val members: Map<String, Any>) {
    private val hooks = members.entries.sortedBy { it.key }.map { it.value }.filterIsInstance<TacHook>()
    private var started = false
    private var disposed = false
    fun start() {
        if (started) return
        started = true
        try { hooks.forEach { if (!it.destroy) it.invoke() } } catch (error: Throwable) { dispose(); throw error }
    }
    fun dispose() {
        if (disposed) return
        disposed = true
        if (started) hooks.forEach { if (it.destroy) try { it.invoke() } catch (_: Exception) { tacEventFailure() } }
        page.dispose()
        NativeCapabilities.dispose(identity)
    }
}
private object TacPageServices {
    const val OWNER_PREFIX = "00000000-0000-4000-a000-"
    const val NODE_LIMIT = 4096
    private val pages = linkedMapOf<String, LinkedHashMap<Pair<String, String>, TacServiceNode>>()
    private var next = 0L
    val size: Int get() = pages.values.sumOf { it.size }
    fun reserved(owner: String): Boolean = owner.lowercase().startsWith(OWNER_PREFIX)
    private fun key(service: String, owner: String): Pair<String, String> =
        service to (if (tacPageSingletons && !tacServicePrototype(service)) "" else owner)
    fun admits(route: String, service: String, owner: String): Boolean {
        val existing = pages[route]?.containsKey(key(service, owner)) == true
        return hasCapacity(existing)
    }
    private fun hasCapacity(existing: Boolean): Boolean = existing || size < NODE_LIMIT
    fun obtain(route: String, service: String, owner: String, props: Map<String, Any?>?): Pair<TacServiceNode, Boolean>? {
        val key = key(service, owner)
        pages[route]?.get(key)?.let { return it to false }
        val shared = key.second.isEmpty()
        val identity = if (shared) OWNER_PREFIX + (++next).toString(16).padStart(12, '0') else owner
        val scope = tacPageScope()
        val members = try { NativeCapabilities.create(identity) { tacServiceMembers(service, props, scope) } }
            catch (error: Exception) { scope.dispose(); NativeCapabilities.dispose(identity); throw error }
            ?: run { scope.dispose(); NativeCapabilities.dispose(identity); return null }
        val node = TacServiceNode(identity, scope, members)
        pages.getOrPut(route) { linkedMapOf() }[key] = node
        return node to true
    }
    fun discard(node: TacServiceNode) {
        pages.values.forEach { page -> page.values.removeAll { it === node } }
        pages.entries.removeAll { it.value.isEmpty() }
        node.dispose()
    }
    fun disposeOwner(owner: String) {
        for (page in pages.values) {
            val owned = page.filterKeys { it.second == owner }
            owned.keys.forEach { page.remove(it) }
            owned.values.reversed().forEach { it.dispose() }
        }
        pages.entries.removeAll { it.value.isEmpty() }
    }
    fun disposeAll() {
        val disposed = pages.keys.sorted().mapNotNull { pages[it] }
        pages.clear()
        disposed.forEach { page -> page.values.reversed().forEach { it.dispose() } }
    }
}
private fun tacEventPublish(name: String, value: Any?, retain: Boolean) {
    val owner = TacNativeState.executing
    if (!tacOnOwnerThread() || owner == null || owner.disposed || TacNativeState.activations[owner.owner] !== owner || owner.queue.size >= 128) { tacEventFailure(); return }
    try {
        fun validate(value: Any?, depth: Int = 0) {
            require(depth <= 64)
            when (value) {
                null, is Unit, is String, is Boolean, is Int, is Long, is Short, is Byte -> Unit
                is Double -> require(value.isFinite())
                is Float -> require(value.isFinite())
                is Iterable<*> -> value.forEach { validate(it, depth + 1) }
                is Map<*, *> -> value.forEach { (key, item) -> require(key is String); validate(item, depth + 1) }
                else -> throw IllegalArgumentException("Non-JSON event value")
            }
        }
        validate(value)
        val json = tacEncode(value)
        require(json.encodeToByteArray().size <= 65536)
        owner.queue.add(Triple(name, json, retain))
    } catch (_: Exception) { tacEventFailure() }
}

private object TacAbsent

private fun tacOperand(node: Map<*, *>, value: Any?, event: String): Any? =
    when (node["k"]) {
        "lit" -> node["v"]
        "event" -> event
        else -> {
            var current: Any? = value
            for (segment in (node["path"] as? List<*> ?: emptyList<Any?>())) {
                current = when (current) {
                    is Map<*, *> -> {
                        val key = when (segment) { is String -> segment; is Number -> tacInt(segment).toString(); else -> null }
                        if (key != null && current.containsKey(key)) current[key] else TacAbsent
                    }
                    is List<*> -> {
                        val index = when (segment) { is Number -> tacInt(segment); is String -> segment.toIntOrNull(); else -> null }
                        if (index != null && index >= 0 && index < current.size) current[index] else TacAbsent
                    }
                    else -> TacAbsent
                }
                if (current === TacAbsent) break
            }
            current
        }
    }

private fun tacNumber(value: Any?): Double? = if (value is Boolean) null else (value as? Number)?.toDouble()

private fun tacTruthy(value: Any?): Boolean =
    when (value) {
        null, TacAbsent -> false
        is Boolean -> value
        is String -> value.isNotEmpty()
        is Number -> value.toDouble() != 0.0 && !value.toDouble().isNaN()
        else -> true
    }

private fun tacIdentical(left: Any?, right: Any?): Boolean {
    if (left === TacAbsent || right === TacAbsent) return left === TacAbsent && right === TacAbsent
    if (left == null || right == null) return left == null && right == null
    if (left is Boolean || right is Boolean) return left is Boolean && right is Boolean && left == right
    if (left is String || right is String) return left is String && right is String && left == right
    val leftNumber = tacNumber(left)
    val rightNumber = tacNumber(right)
    if (leftNumber != null || rightNumber != null) return leftNumber != null && leftNumber == rightNumber
    if (left is List<*> && right is List<*>) {
        return left.size == right.size && left.indices.all { tacIdentical(left[it], right[it]) }
    }
    if (left is Map<*, *> && right is Map<*, *>) {
        return left.size == right.size &&
            left.entries.all { (key, value) -> right.containsKey(key) && tacIdentical(value, right[key]) }
    }
    return false
}

private fun tacOrders(operation: String, left: Any?, right: Any?): Boolean {
    val comparison = when {
        tacNumber(left) != null && tacNumber(right) != null -> tacNumber(left)!!.compareTo(tacNumber(right)!!)
        left is String && right is String -> left.compareTo(right)
        else -> return false
    }
    return when (operation) {
        "lt" -> comparison < 0
        "le" -> comparison <= 0
        "gt" -> comparison > 0
        else -> comparison >= 0
    }
}

private fun tacMatches(matcher: Any?, value: Any?, event: String): Boolean {
    val node = matcher as? Map<*, *> ?: return false
    return when (node["k"]) {
        "and" -> tacMatches(node["l"], value, event) && tacMatches(node["r"], value, event)
        "or" -> tacMatches(node["l"], value, event) || tacMatches(node["r"], value, event)
        "not" -> !tacMatches(node["e"], value, event)
        "truthy" -> tacTruthy(tacOperand(node["e"] as? Map<*, *> ?: return false, value, event))
        "cmp" -> {
            val left = tacOperand(node["l"] as? Map<*, *> ?: return false, value, event)
            val right = tacOperand(node["r"] as? Map<*, *> ?: return false, value, event)
            when (node["op"]) {
                "eq" -> tacIdentical(left, right)
                "ne" -> !tacIdentical(left, right)
                else -> tacOrders(node["op"] as? String ?: "", left, right)
            }
        }
        else -> false
    }
}

private fun tacRepublish(result: Any?, returns: String?) {
    if (returns == null) return
    val items = if (result is Iterable<*>) result.toList() else listOf(result)
    for (item in items.take(32)) {
        if (item == null || item is Unit) continue
        tacEventPublish(returns, tacPlain(item), false)
    }
}

private fun tacDeliver(listener: TacListener, json: String, republish: Boolean) {
    try {
        val result = listener.invoke(tacParseJson(json))
        if (republish) tacRepublish(result, listener.returns)
    } catch (_: Exception) { tacEventFailure() }
}

private fun tacSendEvent(owner: TacActivation, name: String, json: String, retain: Boolean) {
    tacSend("{\"route\":${tacEncode(owner.route)},\"service\":${tacEncode(owner.service)},\"owner\":${tacEncode(owner.owner)},\"name\":${tacEncode(name)},\"value\":$json,\"retain\":$retain}")
}

private fun tacNotifyListeners(owner: TacActivation, name: String, json: String) {
    val value = try { tacParseJson(json) } catch (_: Exception) { null }
    owner.listeners.forEach { listener ->
        if (listener.events.contains(name) && listener.models != null && !tacFits(listener.models, name, value)) {
            tacEventFailure()
        } else if (listener.events.contains(name)) {
            if (listener.condition == null || tacMatches(listener.condition, value, name)) {
                if (listener.asynchronous) {
                    if (owner.deferred.size >= 128) tacEventFailure() else owner.deferred.add(listener to json)
                } else tacDeliver(listener, json, true)
            }
        }
    }
}

private fun tacDrain(owner: TacActivation) {
    var delivered = 0
    while (!owner.disposed) {
        if (owner.deferred.isNotEmpty()) {
            val pending = owner.deferred.toList()
            owner.deferred.clear()
            pending.forEach { (listener, json) -> tacDeliver(listener, json, false) }
            continue
        }
        if (owner.queue.isEmpty()) return
        if (delivered++ == 128) { owner.queue.clear(); tacEventFailure(); return }
        val (name, json, retain) = owner.queue.removeFirst()
        if (tacPageListeners) { tacSendEvent(owner, name, json, retain); continue }
        if (retain) { if (!owner.retained.containsKey(name) && owner.retained.size == 32) { tacEventFailure(); continue }; owner.retained[name] = json }
        tacNotifyListeners(owner, name, json)
        tacSendEvent(owner, name, json, retain)
    }
}
private fun tacDisposeActivation(owner: TacActivation) {
    owner.disposed = true
    owner.queue.clear(); owner.retained.clear(); owner.deferred.clear()
}
private fun tacDisposeOwner(id: String) {
    tacCancelTasks(id)
    val owner = TacNativeState.activations.remove(id)
    if (owner != null && TacNativeState.executing === owner) TacNativeState.executing = null
    owner?.let { tacDisposeActivation(it) }
    TacPageServices.disposeOwner(id)
}
private fun tacDisposeAll() {
    tacCancelTasks(null)
    val owners = TacNativeState.activations.values.toList()
    TacNativeState.activations.clear(); TacNativeState.executing = null
    owners.forEach { tacDisposeActivation(it) }
    TacPageServices.disposeAll()
}

private fun tacText(value: Any?): String = if (value == null) "" else value.toString()

private fun tacObject(value: Any?): Map<String, Any?>? =
    (value as? Map<*, *>)?.entries?.associate { it.key.toString() to it.value }

private fun tacEncode(value: Any?): String =
    when (value) {
        null, is Unit -> "null"
        is Boolean -> value.toString()
        is Int, is Long, is Short, is Byte -> value.toString()
        is Double -> if (!value.isFinite()) "null" else if (value == kotlin.math.floor(value))
            value.toLong().toString() else value.toString()
        is Float -> tacEncode(value.toDouble())
        is String -> tacQuote(value)
        is Iterable<*> -> value.joinToString(",", "[", "]") { tacEncode(it) }
        is Map<*, *> -> value.entries.joinToString(",", "{", "}") { tacEncode(it.key.toString()) + ":" + tacEncode(it.value) }
        else -> tacQuote(value.toString())
    }

private fun tacBoundedJSON(raw: String): Boolean {
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

private fun tacOwnerId(owner: String): Boolean =
    owner.length == 36 && owner.lowercase().withIndex().all { (index, character) ->
        if (index == 8 || index == 13 || index == 18 || index == 23) character == '-' else character in "0123456789abcdef"
    }

private fun tacRequiredValue(props: Map<String, Any?>?, name: String): Any =
    props?.get(name) ?: throw IllegalArgumentException(name)

private fun tacAssignProperties(activation: TacActivation, props: Map<String, Any?>?) {
    if (props == null) return
    val required = tacServiceRequired(activation.service)
    for ((property, value) in props) {
        val field = activation.members[property]
        if (("*" in required || property in required) && field is TacField) field.write?.invoke(value)
    }
}

private fun tacDescribeListener(key: String, listener: TacListener): Map<String, Any?> =
    linkedMapOf(
        "models" to listener.models,
        "key" to key,
        "events" to listener.events,
        "replay" to listener.replay,
        "order" to listener.order,
        "asynchronous" to listener.asynchronous,
        "condition" to listener.condition,
    )

private fun tacDescribeStream(stream: TacStream): Map<String, Any?> =
    linkedMapOf(
        "client" to stream.client,
        "path" to stream.path,
        "event" to stream.event,
        "socket" to stream.socket,
        "models" to stream.models?.let { tacParseJson(it) },
    )

private fun tacDescribe(members: Map<String, Any>): String {
    val fields = members.entries.filter { it.value is TacField }.map { it.key }
    val methods = members.entries.filter { it.value is TacMethod || it.value is TacTask }.map { it.key }
    val tasks = members.entries.filter { it.value is TacTask && tacTaskHost() }.map { it.key }
    if (!tacPageListeners) return "{\"value\":{\"fields\":${tacEncode(fields)},\"methods\":${tacEncode(methods)}}}"
    val listeners = members.entries.mapNotNull { (key, member) -> (member as? TacListener)?.let { tacDescribeListener(key, it) } }
    val streams = members.values.mapNotNull { (it as? TacStream)?.let(::tacDescribeStream) }
    return "{\"value\":{\"fields\":${tacEncode(fields)},\"methods\":${tacEncode(methods)},\"asyncMethods\":${tacEncode(tasks)},\"listeners\":${tacEncode(listeners)},\"streams\":${tacEncode(streams)}}}"
}

private fun tacReceive(activation: TacActivation, key: String, value: Any?): String {
    val listener = activation.members[key] as? TacListener
        ?: return "{\"error\":${tacQuote("Unknown companion listener: $key")}}"
    val result = try { listener.invoke(value) } catch (_: Exception) { return "{\"error\":\"Native event listener failed.\"}" }
    if (!listener.asynchronous) tacRepublish(result, listener.returns)
    return "{\"value\":null}"
}

private fun tacInvokeOwned(request: String): String {
    if (TacNativeState.handling) return "{\"error\":\"Native owner is busy.\"}"
    TacNativeState.handling = true
    return try { tacHandle(request) } finally { TacNativeState.executing?.let { tacDrain(it) }; TacNativeState.executing = null; TacNativeState.handling = false }
}
private fun tacTooLarge(subject: String, size: Int, limit: Int): String =
    "The $subject is $size bytes. The limit is $limit bytes. Pass less data to the service."
private fun tacHandle(request: String): String {
    return try {
        val size = request.encodeToByteArray().size
        require(size <= TacNativeState.START_UP_REQUEST_LIMIT) {
            tacTooLarge("request", size, TacNativeState.START_UP_REQUEST_LIMIT)
        }
        require(tacBoundedJSON(request)) { "Invalid companion request." }
        val parsed = tacObject(tacParseJson(request)) ?: throw IllegalArgumentException("Invalid companion request.")
        val operation = tacText(parsed["op"])
        require(operation == "init" || size <= TacNativeState.MEMBER_REQUEST_LIMIT) {
            tacTooLarge("request to " + tacText(parsed["name"]).ifEmpty { operation }, size, TacNativeState.MEMBER_REQUEST_LIMIT)
        }
        if (operation == "host-dispose") { tacDisposeAll(); return "{\"value\":null}" }
        val route = tacText(parsed["route"])
        val owner = tacText(parsed["owner"])
        require(tacOwnerId(owner) && !TacPageServices.reserved(owner)) { "Missing native owner identity." }
        val service = tacText(parsed["service"])
        require(service.isNotEmpty()) { "Missing companion name." }
        val existing = TacNativeState.activations[owner]
        if (operation == "init" && (existing == null || existing.route != route || existing.service != service)) {
            if (existing != null) tacDisposeOwner(owner)
            check(TacNativeState.activations.size < 256) { "Too many native activations." }
            check(TacPageServices.admits(route, service, owner)) { "Too many native services." }
            val props = tacObject(parsed["props"])
            val (node, fresh) = try { TacPageServices.obtain(route, service, owner, props) }
                catch (_: Exception) { return "{\"error\":\"Native service is missing a required property or it has the wrong type.\",\"code\":\"TY_NATIVE_REQUIRED\"}" }
                ?: return "{\"error\":\"Unknown native service.\",\"code\":\"TY_NATIVE_SERVICE\"}"
            val activation = TacActivation(route, service, owner, node.members)
            TacNativeState.activations[owner] = activation; TacNativeState.executing = activation
            try { tacAssignProperties(activation, props); node.start(); tacDrain(activation) }
            catch (error: Throwable) { if (fresh) TacPageServices.discard(node); tacDisposeOwner(owner); throw error }
        }
        val activation = TacNativeState.activations[owner]
        require(activation != null && activation.route == route && activation.service == service) { "Stale native owner." }
        if (operation == "dispose") { tacDisposeOwner(owner); return "{\"value\":null}" }
        TacNativeState.executing = activation
        val tac = activation.members
        val name = tacText(parsed["name"])
        when {
            operation == "init" -> tacDescribe(tac)
            operation == "event" && tacPageListeners -> tacReceive(activation, name, parsed["value"])
            else -> when (val member = tac[name]) {
                null -> "{\"error\":${tacQuote("Unknown companion member: $name")}}"
                is TacField -> when {
                    operation == "get" -> "{\"value\":${tacEncode(tacPlain(member.read()))}}"
                    operation == "set" && member.write != null -> {
                        member.write.invoke(parsed["value"])
                        "{\"value\":null}"
                    }
                    operation == "set" -> "{\"error\":${tacQuote("Companion field is read-only: $name")}}"
                    else -> "{\"error\":${tacQuote("Companion member does not support $operation: $name")}}"
                }
                is TacMethod -> if (operation == "call") {
                    val arguments = parsed["args"] as? List<*> ?: emptyList<Any?>()
                    "{\"value\":${tacEncode(tacPlain(member.invoke(arguments)))}}"
                } else "{\"error\":${tacQuote("Companion member does not support $operation: $name")}}"
                is TacTask -> if (operation == "call") {
                    tacBeginTask(activation, member, parsed["args"] as? List<*> ?: emptyList<Any?>())
                } else "{\"error\":${tacQuote("Companion member does not support $operation: $name")}}"
                else -> "{\"error\":\"Unknown companion member kind.\"}"
            }
        }
    } catch (error: Throwable) {
        tacFailureAnswer(error)
    }
}
