object BehaviorTasks {
    private const val STILL_WAITING = "The test case still waits. A test adapter must answer at once."

    fun finish(work: suspend () -> Unit) {
        var outcome: Result<Unit>? = null
        work.startCoroutine(kotlin.coroutines.Continuation(kotlin.coroutines.EmptyCoroutineContext) { outcome = it })
        checkNotNull(outcome) { STILL_WAITING }.getOrThrow()
    }
}

class BehaviorSocketFailure(val code: String) : RuntimeException("$code: A test unit has no page, so the socket message was not sent.")

suspend fun behaviorSocketSend(client: String, message: Any) { throw BehaviorSocketFailure("HTTP_MISSING") }
