@Service
class StreamService {
  @Needs
  final TicksClient ticksClient;
  List<Tick> ticks = [];

  @OnEvent
  void addTick(Tick tick) {
    ticks = [...ticks.skip(ticks.length > 127 ? ticks.length - 127 : 0), tick];
  }
}
