import java.io.OutputStreamWriter

class KotlinFyloRepository {
    private val root = System.getenv("FYLO_ROOT") ?: "${System.getProperty("user.dir")}/db"
    private val executable = System.getenv("FYLO_EXEC_PATH")

    fun patchSample(requestId: String): Map<String, Any?> {
        val collection = "language-route-events"
        machine(mapOf("op" to "createCollection", "collection" to collection))
        machine(mapOf(
            "op" to "batchPutData",
            "collection" to collection,
            "batch" to listOf(
                mapOf("language" to "kotlin", "source" to "fylo.exec", "requestId" to requestId, "group" to "patch"),
                mapOf("language" to "kotlin", "source" to "fylo.exec", "requestId" to requestId, "group" to "patch"),
            ),
        ))
        machine(mapOf(
            "op" to "patchDocs",
            "collection" to collection,
            "update" to mapOf(
                "\$set" to mapOf("source" to "patched"),
                "\$where" to mapOf("\$ops" to listOf(mapOf("language" to mapOf("\$eq" to "kotlin")))),
            ),
        ))
        return mapOf("collection" to collection, "operations" to listOf("createCollection", "batchPutData", "patchDocs"), "resultCount" to "3")
    }

    @Suppress("UNCHECKED_CAST")
    private fun machine(request: Map<String, Any?>): Any? {
        val command = if (executable.isNullOrBlank()) {
            listOf("bunx", "--bun", "fylo.exec", "exec", "--request", "-", "--root", root)
        } else {
            listOf(executable, "exec", "--request", "-", "--root", root)
        }
        val process = ProcessBuilder(command).start()
        OutputStreamWriter(process.outputStream).use { it.write(YonJson.stringify(request)) }
        val stdout = process.inputStream.bufferedReader().readText()
        val stderr = process.errorStream.bufferedReader().readText()
        val code = process.waitFor()
        if (code != 0) error(if (stderr.isNotBlank()) stderr else stdout)
        val response = YonJson.parse(if (stdout.isBlank()) "{}" else stdout) as Map<String, Any?>
        if (response["ok"] != true) error("fylo.exec returned an error")
        return response["result"]
    }
}
