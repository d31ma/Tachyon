[HTTPClient("/ticks")]
public interface TicksClient
{
    [StreamRequest]
    void Receive(Models.Tick tick);
}
