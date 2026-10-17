import models.Tick

@HTTPClient("/ticks")
interface TicksClient {
    @StreamRequest
    fun streamTicks(tick: Tick)
}
