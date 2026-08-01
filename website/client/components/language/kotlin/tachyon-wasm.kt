var count: Int = 6
val label: String = "from Kotlin"

fun doubled(): Int = count * 2

val tac =
    mapOf(
        "count" to TacField({ count }, { count = tacInt(it) }),
        "label" to TacField({ label }),
        "doubled" to TacMethod { doubled() },
    )
