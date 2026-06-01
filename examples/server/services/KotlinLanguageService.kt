class KotlinLanguageService {
    private val fyloRepository = KotlinFyloRepository()

    fun describe(request: Map<String, Any?>): Map<String, Any?> {
        @Suppress("UNCHECKED_CAST")
        val context = request["context"] as? Map<String, Any?>
        val requestId = (context?.get("requestId") as? String) ?: "unknown"

        return mapOf(
            "language" to "kotlin",
            "message" to "Hello from Kotlin!",
            "requestId" to requestId,
            "fylo" to fyloRepository.patchSample(requestId),
        )
    }
}
