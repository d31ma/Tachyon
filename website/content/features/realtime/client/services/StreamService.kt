import models.Tick

@Service
class StreamService {
    @Needs
    private val ticksClient: TicksClient
    var ticks: List<Tick> = emptyList()

    @OnEvent
    fun addTick(tick: Tick) {
        ticks = (ticks + tick).takeLast(tickLimit())
    }

    private fun tickLimit(): Int = 128
}
