@HTTPClient("/ticks")
abstract interface class TicksClient {
  @StreamRequest
  void streamTicks(Tick tick);
}
