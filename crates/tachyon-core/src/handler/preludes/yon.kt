// Appended after a `yon.kt` handler. Everything below is the protocol.
//
// The author writes a class and methods. Reading standard input, dispatching
// on the method and writing the response envelope is Tachyon's half.
//
// Kotlin's standard library has no JSON, and `org.json` is on Android's
// classpath rather than on a bare `kotlinc` one, so a parser is supplied here
// instead of asked for. `Json.parse` and `Json.write` are simply in scope.

@Target(AnnotationTarget.CLASS)
annotation class Controller

@Target(AnnotationTarget.CLASS)
annotation class Service

@Target(AnnotationTarget.CLASS)
annotation class Repository

@Target(AnnotationTarget.CLASS)
annotation class Client

@Target(AnnotationTarget.CLASS)
annotation class Delegate

/**
 * Marks a method as a proxy for a program Yon does not run.
 *
 * RUNTIME retention, unlike the five above: those are read by Tachyon before
 * the compiler runs and nothing reads them back, where this one is read by the
 * dispatch below, which runs after it. Kotlin is one of the four languages
 * here whose annotations cannot do the work themselves — an annotation is
 * inert, so the dispatch checks for it and the annotated body is never called.
 */
@Target(AnnotationTarget.FUNCTION)
@Retention(AnnotationRetention.RUNTIME)
annotation class Relay(vararg val command: String)

/**
 * Marks a method that answers more than once.
 *
 * SOURCE retention, unlike `@Relay` above: Tachyon reads this before the
 * compiler runs, to tell the server which routes to read frames from, and the
 * dispatch below never asks for it — a `Sequence` is what it looks at. The two
 * are checked against each other at build time, so they cannot disagree.
 */
@Target(AnnotationTarget.FUNCTION)
@Retention(AnnotationRetention.SOURCE)
annotation class Stream

/** JSON, in the box, because Kotlin does not put it there. */
object Json {
    /** Reads one JSON document into maps, lists, strings, numbers and booleans. */
    fun parse(source: String?): Any? {
        val text = if (source.isNullOrBlank()) "{}" else source
        return Reader(text).value()
    }

    /** Writes one value as JSON. */
    fun write(value: Any?): String = when (value) {
        null -> "null"
        is String -> quote(value)
        is Boolean -> value.toString()
        is Number -> value.toString()
        is Map<*, *> -> value.entries.joinToString(",", "{", "}") {
            "${quote(it.key.toString())}:${write(it.value)}"
        }
        is Iterable<*> -> value.joinToString(",", "[", "]") { write(it) }
        // Anything else crosses as its text, which is what a handler returning
        // an unforeseen type would want to see at the other end.
        else -> quote(value.toString())
    }

    fun quote(text: String): String {
        val out = StringBuilder("\"")
        for (character in text) {
            when (character) {
                '"' -> out.append("\\\"")
                '\\' -> out.append("\\\\")
                '\n' -> out.append("\\n")
                '\r' -> out.append("\\r")
                '\t' -> out.append("\\t")
                else ->
                    if (character.code < 0x20) out.append("\\u%04x".format(character.code))
                    else out.append(character)
            }
        }
        return out.append('"').toString()
    }

    private class Reader(private val source: String) {
        private var at = 0

        fun value(): Any? {
            skip()
            if (at >= source.length) return null
            return when (source[at]) {
                '{' -> obj()
                '[' -> list()
                '"' -> string()
                't' -> { at += 4; true }
                'f' -> { at += 5; false }
                'n' -> { at += 4; null }
                else -> number()
            }
        }

        private fun skip() {
            while (at < source.length && source[at].isWhitespace()) at += 1
        }

        private fun obj(): Map<String, Any?> {
            val entries = LinkedHashMap<String, Any?>()
            at += 1
            while (at < source.length) {
                skip()
                when (source[at]) {
                    '}' -> { at += 1; return entries }
                    ',' -> { at += 1; continue }
                }
                val key = string()
                skip()
                if (at < source.length && source[at] == ':') at += 1
                entries[key] = value()
            }
            return entries
        }

        private fun list(): List<Any?> {
            val values = ArrayList<Any?>()
            at += 1
            while (at < source.length) {
                skip()
                when (source[at]) {
                    ']' -> { at += 1; return values }
                    ',' -> { at += 1; continue }
                }
                values.add(value())
            }
            return values
        }

        private fun string(): String {
            val out = StringBuilder()
            if (at >= source.length || source[at] != '"') return out.toString()
            at += 1
            while (at < source.length) {
                val character = source[at]
                at += 1
                if (character == '"') break
                if (character != '\\') { out.append(character); continue }
                val escaped = source[at]
                at += 1
                when (escaped) {
                    'n' -> out.append('\n')
                    'r' -> out.append('\r')
                    't' -> out.append('\t')
                    'b' -> out.append('\b')
                    'u' -> {
                        out.append(source.substring(at, at + 4).toInt(16).toChar())
                        at += 4
                    }
                    else -> out.append(escaped)
                }
            }
            return out.toString()
        }

        private fun number(): Any {
            val start = at
            while (at < source.length && source[at] in "0123456789+-.eE") at += 1
            val digits = source.substring(start, at)
            // Whole numbers cross as longs, because a handler counting things
            // wrote an integer and expects one back.
            return digits.toLongOrNull() ?: (digits.toDoubleOrNull() ?: 0)
        }
    }
}

/** One request, as the protocol delivers it. */
class YonRequest(parsed: Any?) {
    @Suppress("UNCHECKED_CAST")
    private val raw: Map<String, Any?> = parsed as? Map<String, Any?> ?: emptyMap()

    /** The HTTP method, upper case. */
    fun method(): String = raw["method"]?.toString() ?: ""

    /** The route as matched, with its parameters still in it. */
    fun route(): String = raw["route"]?.toString() ?: ""

    /** One bound route parameter, or an empty string when there is none. */
    @Suppress("UNCHECKED_CAST")
    fun parameter(name: String): String =
        (raw["parameters"] as? Map<String, Any?>)?.get(name)?.toString() ?: ""

    /** The request body as text, empty when none was sent. */
    @Suppress("UNCHECKED_CAST")
    fun body(): String = (raw["body"] as? Map<String, Any?>)?.get("data")?.toString() ?: ""

    /** The whole request, for anything the accessors do not cover. */
    fun raw(): Map<String, Any?> = raw
}

/** One response, built the way the other layers build their return values. */
class YonResponse private constructor(private var status: Int, private val body: String) {
    private val headers = LinkedHashMap<String, MutableList<String>>()

    companion object {
        /** A JSON body with a 200. The common case, so it is the short one. */
        fun json(body: String): YonResponse {
            val response = YonResponse(200, body)
            response.headers["content-type"] = mutableListOf("application/json")
            return response
        }

        /** A response with no body, for a 204 or a redirect. */
        fun empty(status: Int): YonResponse = YonResponse(status, "")
    }

    fun status(value: Int): YonResponse {
        status = value
        return this
    }

    /** Replaces every header at once, for a delegate answering with its own.
     * Without this the assumed content type would survive alongside the one
     * the delegate actually set. */
    fun headers(replacement: Map<String, List<String>>): YonResponse {
        headers.clear()
        replacement.forEach { (name, values) -> headers[name] = values.toMutableList() }
        return this
    }

    fun header(name: String, value: String): YonResponse {
        // A header value is a list, because one header may repeat.
        headers.getOrPut(name) { mutableListOf() }.add(value)
        return this
    }

    fun envelope(): String =
        Json.write(mapOf("status" to status, "headers" to headers, "body" to body))
}

/** Work handed to a language Yon does not run. */
object Yon {
    private const val MAX_RELAY_STDOUT_BYTES = 16 * 1024 * 1024
    private const val MAX_RELAY_STDERR_BYTES = 64 * 1024
    private data class RelayOutput(val bytes: ByteArray, val overflow: Boolean)

    private fun drain(pipe: java.io.InputStream, limit: Int): RelayOutput {
        val kept = java.io.ByteArrayOutputStream(minOf(limit, 8192))
        val chunk = ByteArray(8192)
        var overflow = false
        pipe.use {
            while (true) {
                val count = try { it.read(chunk) } catch (_: java.io.IOException) {
                    overflow = true
                    break
                }
                if (count < 0) break
                val remaining = maxOf(0, limit - kept.size())
                kept.write(chunk, 0, minOf(count, remaining))
                overflow = overflow || count > remaining
            }
        }
        return RelayOutput(kept.toByteArray(), overflow)
    }

    private fun killTree(child: Process) {
        child.descendants().forEach { it.destroyForcibly() }
        child.destroyForcibly()
    }

    /**
     * Runs a handler written in a language Yon does not run.
     *
     * Yon runs the eight languages that can declare a layer. Go, Ruby, Elixir
     * and the rest cannot, so they are not routes — but they are still
     * programs, and a program that speaks Handler Protocol v1 on standard
     * input and output is exactly what Yon spawns anyway.
     *
     * The command is explicit rather than inferred from the file name: a
     * compiled language has no interpreter to infer. The working directory is
     * the project root, so a project-relative path reads as written.
     */
    @JvmStatic
    fun relay(command: List<String>, request: YonRequest): YonResponse {
        command.firstOrNull() ?: return failed("A delegate command cannot be empty.")
        return try {
            // No shell: a route parameter that reached a command line would be
            // an injection.
            val child = ProcessBuilder(command).start()
            val stdoutPipe = child.inputStream
            val stderrPipe = child.errorStream
            val stdoutTask = java.util.concurrent.CompletableFuture.supplyAsync {
                drain(stdoutPipe, MAX_RELAY_STDOUT_BYTES)
            }
            val stderrTask = java.util.concurrent.CompletableFuture.supplyAsync {
                drain(stderrPipe, MAX_RELAY_STDERR_BYTES)
            }
            // Closed before the output is read, so the child sees end of input
            // rather than waiting for more while this side waits for an answer.
            child.outputStream.use { it.write(Json.write(request.raw()).toByteArray()) }
            val requested = (request.raw()["deadline_ms"] as? Number)?.toLong() ?: 30_000L
            val timeout = requested.coerceIn(1L, 300_000L)
            if (!child.waitFor(timeout, java.util.concurrent.TimeUnit.MILLISECONDS)) {
                killTree(child)
                child.waitFor(1, java.util.concurrent.TimeUnit.SECONDS)
                return failed("Delegate invocation failed.")
            }
            val stdout = stdoutTask.get(1, java.util.concurrent.TimeUnit.SECONDS)
            val stderr = stderrTask.get(1, java.util.concurrent.TimeUnit.SECONDS)
            if (child.exitValue() != 0 || stdout.overflow || stderr.overflow) {
                failed("Delegate invocation failed.")
            } else {
                envelope(stdout.bytes.decodeToString())
            }
        } catch (error: java.io.IOException) {
            failed("Delegate invocation failed.")
        } catch (interrupted: InterruptedException) {
            Thread.currentThread().interrupt()
            failed("Delegate invocation failed.")
        } catch (_: java.util.concurrent.ExecutionException) {
            failed("Delegate invocation failed.")
        } catch (_: java.util.concurrent.TimeoutException) {
            failed("Delegate invocation failed.")
        }
    }

    private fun envelope(stdout: String): YonResponse {
        val parsed = Json.parse(stdout) as? Map<*, *>
            ?: return failed("Delegate returned an invalid response.")
        var response = YonResponse.json(parsed["body"]?.toString() ?: "")
        (parsed["status"] as? Number)?.let { response = response.status(it.toInt()) }
        // The headers the delegate set replace the assumed content type,
        // because a delegate that answered with a header meant that header.
        val written = parsed["headers"] as? Map<*, *>
        if (!written.isNullOrEmpty()) {
            response = response.headers(emptyMap())
            written.forEach { (name, values) ->
                val list = values as? List<*> ?: listOf(values)
                list.forEach { response = response.header(name.toString(), it.toString()) }
            }
        }
        return response
    }

    /**
     * A delegate that could not be run answers 502, the same as any other
     * upstream that did not reply. Delegate stderr and process errors are
     * diagnostic-only and never become client response data.
     */
    private fun failed(reason: String): YonResponse =
        YonResponse.json(Json.write(mapOf("error" to reason))).status(502)
}

/**
 * One streamed frame on standard output.
 *
 * A stream is length-prefixed where a single response is not: the reader has
 * to know where each frame ends, because more follow. `[4-byte big-endian
 * length][UTF-8 JSON]` is the shape the two adapters already write, and the
 * reader that consumes it never asks which language wrote it.
 */
object YonStream {
    /** The reader refuses a larger frame, so refusing it here says why. */
    private const val MAX_FRAME_BYTES = 16 * 1024 * 1024

    fun event(requestId: String, value: Any?): Boolean = frame(
        requestId,
        mapOf(
            "protocol_version" to 1,
            "kind" to "event",
            "request_id" to requestId,
            "body" to mapOf("encoding" to "utf8", "data" to Json.write(value)),
        ),
    )

    /**
     * An error frame ends the stream. The reader turns it into a failure
     * rather than presenting it as data, which is the difference between a
     * handler that stopped and one that had nothing more to say.
     */
    fun failure(requestId: String, message: String) {
        frame(
            requestId,
            mapOf(
                "protocol_version" to 1,
                "kind" to "response",
                "request_id" to requestId,
                "status" to 500,
                "headers" to emptyMap<String, List<String>>(),
                "error" to mapOf(
                    "code" to "TY2204",
                    "message" to message,
                    "retryable" to false,
                ),
            ),
        )
    }

    private fun frame(requestId: String, frame: Map<String, Any?>): Boolean {
        val payload = Json.write(frame).toByteArray(Charsets.UTF_8)
        if (payload.size > MAX_FRAME_BYTES) {
            val refusal = Json.write(
                mapOf(
                    "protocol_version" to 1,
                    "kind" to "response",
                    "request_id" to requestId,
                    "status" to 500,
                    "headers" to emptyMap<String, List<String>>(),
                    "error" to mapOf(
                        "code" to "TY2203",
                        "message" to "Streamed event exceeds the protocol frame limit.",
                        "retryable" to false,
                    ),
                ),
            ).toByteArray(Charsets.UTF_8)
            write(refusal)
            return false
        }
        write(payload)
        return true
    }

    private fun write(payload: ByteArray) {
        val out = System.out
        // The prefix is written as bytes rather than through a writer, which
        // would apply the console encoding to four bytes that are already
        // exactly what they should be.
        out.write(
            byteArrayOf(
                (payload.size ushr 24).toByte(),
                (payload.size ushr 16).toByte(),
                (payload.size ushr 8).toByte(),
                payload.size.toByte(),
            ),
        )
        out.write(payload)
        // Flushed per frame: a stream whose events arrive together at the end
        // is a slow response wearing a stream's clothes.
        out.flush()
    }
}

/** The entry point Tachyon supplies, so the handler does not write one. */
fun main() {
    val raw = generateSequence(::readLine).joinToString("\n")
    val request = YonRequest(Json.parse(raw))
    // Found by reflection rather than by a fixed list, so a method the handler
    // does not declare answers 405 without being written.
    val method = __YON_CONTROLLER__::class.java.methods.firstOrNull {
        it.name == request.method() && it.parameterCount == 1
    }
    val result: Any? = if (method == null) {
        YonResponse.empty(405)
    } else {
        try {
            method.invoke(__YON_CONTROLLER__, request)
        } catch (failed: java.lang.reflect.InvocationTargetException) {
            val cause = failed.cause ?: failed
            YonResponse.json(Json.write(mapOf("error" to cause.message))).status(500)
        }
    }

    // A method that yields returns a Sequence, and a Sequence is a stream:
    // each value becomes one frame and end of stream is end of process,
    // because the reader takes EOF as the close.
    //
    // `@Stream` on the method is what told the server to read frames at all,
    // and Tachyon refuses the two to disagree — so a Sequence here means the
    // other end is already reading them.
    //
    // Sequence and not Flow: a Flow needs kotlinx-coroutines, which is not on
    // a bare kotlinc classpath, for the same reason org.json is not.
    if (result is Sequence<*>) {
        val id = request.raw()["request_id"]?.toString() ?: ""
        try {
            for (event in result) {
                if (!YonStream.event(id, event)) {
                    return
                }
            }
        } catch (failed: Throwable) {
            YonStream.failure(id, failed.message ?: failed.toString())
        }
        return
    }
    print((result as YonResponse).envelope())
}
