[Service]
public sealed class StreamService
{
    private const int TickLimit = 128;

    [Needs]
    private readonly TicksClient _ticksClient;

    public List<Models.Tick> Ticks = new();

    [OnEvent]
    public void AddTick(Models.Tick tick)
    {
        Ticks.Add(tick);
        if (Ticks.Count > TickLimit) Ticks.RemoveRange(0, Ticks.Count - TickLimit);
    }
}
