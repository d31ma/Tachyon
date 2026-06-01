class Handler {
    companion object {
        fun GET(request: Map<String, Any?>): Any? {
            return when (statusCode(request)) {
                "411" -> mapOf("code" to "411", "detail" to "length required")
                "412" -> mapOf("code" to "412", "detail" to "precondition failed")
                "413" -> mapOf("code" to "413", "detail" to "payload too large")
                "414" -> mapOf("code" to "414", "detail" to "uri too long")
                "415" -> mapOf("code" to "415", "detail" to "unsupported media type")
                else -> KotlinLanguageService().describe(request)
            }
        }

        @Suppress("UNCHECKED_CAST")
        private fun statusCode(request: Map<String, Any?>): String {
            val query = request["query"] as? Map<String, Any?> ?: return ""
            return when (val code = query["code"]) {
                is String -> code
                is Number -> code.toLong().toString()
                else -> ""
            }
        }
    }
}
