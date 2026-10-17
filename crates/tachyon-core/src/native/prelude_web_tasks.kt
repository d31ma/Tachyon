
@JsFun("(ticket, text) => globalThis[Symbol.for('tachyon.kotlin.host')].httpStart(ticket, text)")
private external fun tacHostHttpStart(ticket: Int, text: String)

@JsFun("(ticket) => globalThis[Symbol.for('tachyon.kotlin.host')].httpCancel(ticket)")
private external fun tacHostHttpCancel(ticket: Int)

@JsFun("(ticket, owner, text) => globalThis[Symbol.for('tachyon.kotlin.host')].socket(ticket, owner, text)")
private external fun tacHostSocket(ticket: Int, owner: String, text: String)

@JsFun("(task, text) => globalThis[Symbol.for('tachyon.kotlin.host')].taskDone(task, text)")
private external fun tacHostTaskDone(task: Int, text: String)

private fun tacTaskHost(): Boolean = true

private fun tacBeginTask(activation: TacActivation, task: TacTask, arguments: List<Any?>): String =
    TacTasks.begin(activation.owner, task, arguments)

private fun tacCancelTasks(owner: String?) { TacTasks.cancel(owner) }

private suspend fun tacHttpExchange(request: String): String = TacHttpTickets.exchange(request)

private class TacRunningTask(val id: Int, val owner: String, private val code: (Throwable) -> String?) : kotlin.coroutines.Continuation<Any?> {
    override val context: kotlin.coroutines.CoroutineContext = kotlin.coroutines.EmptyCoroutineContext
    var answer: String? = null
    var cancel: () -> Unit = {}

    override fun resumeWith(result: Result<Any?>) {
        answer = result.fold({ "{\"value\":${tacEncode(tacPlain(it))}}" }, { failed(it) })
    }

    private fun failed(error: Throwable): String {
        val message = tacQuote(error.message ?: "Kotlin service task failed.")
        val named = code(error) ?: return "{\"error\":$message}"
        return "{\"error\":$message,\"code\":${tacQuote(named)}}"
    }
}

private object TacTasks {
    private const val MOST_TASKS = 256
    private const val CANCELLED = "{\"error\":\"The service was removed before its task finished.\",\"code\":\"CANCELLED\"}"
    private const val TOO_MANY = "{\"error\":\"The page runs too many service tasks at once.\",\"code\":\"CAPACITY\"}"
    private var last = 0
    private val parked = linkedMapOf<Int, TacRunningTask>()
    var current: TacRunningTask? = null

    fun begin(owner: String, task: TacTask, arguments: List<Any?>): String {
        if (parked.size >= MOST_TASKS) return TOO_MANY
        last += 1
        val running = TacRunningTask(last, owner, task.code)
        val work: suspend () -> Any? = { task.invoke(arguments) }
        current = running
        try { work.startCoroutine(running) } finally { current = null }
        val answer = running.answer
        if (answer != null) return answer
        parked[running.id] = running
        return "{\"task\":${running.id}}"
    }

    fun resume(running: TacRunningTask, step: () -> Unit) {
        if (parked[running.id] !== running) return
        val activation = TacNativeState.activations[running.owner]
        TacNativeState.handling = true
        TacNativeState.executing = activation
        current = running
        try { step() } finally {
            current = null
            activation?.let { tacDrain(it) }
            TacNativeState.executing = null
            TacNativeState.handling = false
        }
        val answer = running.answer ?: return
        parked.remove(running.id)
        tacHostTaskDone(running.id, answer)
    }

    fun cancel(owner: String?) {
        val removed = parked.values.filter { owner == null || it.owner == owner }
        for (running in removed) {
            parked.remove(running.id)
            running.cancel()
            tacHostTaskDone(running.id, CANCELLED)
        }
    }
}

private object TacHttpTickets {
    private const val OUTSIDE_TASK = "{\"error\":{\"code\":\"HTTP_REQUEST\"}}"
    private var last = 0
    private val waiting = HashMap<Int, Pair<TacRunningTask, kotlin.coroutines.Continuation<String>>>()

    suspend fun exchange(request: String): String =
        awaited({ ticket -> tacHostHttpCancel(ticket) }) { ticket, _ -> tacHostHttpStart(ticket, request) }

    suspend fun awaited(cancel: (Int) -> Unit, start: (Int, String) -> Unit): String {
        val running = TacTasks.current ?: return OUTSIDE_TASK
        return kotlin.coroutines.suspendCoroutine { continuation ->
            last += 1
            val ticket = last
            waiting[ticket] = running to continuation
            running.cancel = { waiting.remove(ticket); cancel(ticket) }
            start(ticket, running.owner)
        }
    }

    fun settle(ticket: Int, answer: String) {
        val (running, continuation) = waiting.remove(ticket) ?: return
        running.cancel = {}
        TacTasks.resume(running) { continuation.resumeWith(Result.success(answer)) }
    }
}

@OptIn(kotlin.js.ExperimentalJsExport::class)
@JsExport
fun tacHttpSettle(ticket: Int, answer: String) { TacHttpTickets.settle(ticket, answer) }

private class TacSocketFailure(val code: String, message: String) : RuntimeException("$code: $message")

private object TacSockets {
    private const val MOST_BYTES = 65536
    private val CODES = setOf("HTTP_MISSING", "HTTP_REQUEST", "CANCELLED")

    suspend fun send(client: String, message: Any): Unit {
        val text = try { tacEncode(tacPlain(message)) } catch (_: RuntimeException) { null }
        if (text == null || text.encodeToByteArray().size > MOST_BYTES) {
            throw TacSocketFailure("HTTP_BODY", "A socket message is JSON of at most 64 KiB.")
        }
        val request = "{\"client\":${tacQuote(client)},\"message\":$text}"
        val answer = TacHttpTickets.awaited({}) { ticket, owner -> tacHostSocket(ticket, owner, request) }
        val parsed = tacObject(try { tacParseJson(answer) } catch (_: RuntimeException) { null })
        val failure = parsed?.get("error") as? Map<*, *>
        if (failure == null && parsed?.containsKey("value") == true) return
        val code = (failure?.get("code") as? String)?.takeIf { it in CODES } ?: "HTTP_NETWORK"
        throw TacSocketFailure(code, "The socket message was not sent.")
    }
}

private suspend fun behaviorSocketSend(client: String, message: Any) { TacSockets.send(client, message) }
