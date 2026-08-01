// A Kotlin browser companion, written as plain Kotlin. There is no tac_invoke
// here and no subset of the language: this is compiled by kotlinc-js targeting
// wasm, and the prelude the build appends carries the ABI of ADR 0011.
//
// Members are declared because Kotlin/Wasm has no reflection to discover them
// with, and the host must know a field from a method.

var count: Int = 6
val label: String = "from kotlin"

fun doubled(): Int = count * 2

val tac =
    mapOf(
        "count" to TacField({ count }, { count = tacInt(it) }),
        "label" to TacField({ label }),
        "doubled" to TacMethod { doubled() },
    )
