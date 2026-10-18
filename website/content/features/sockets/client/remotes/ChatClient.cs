[HTTPClient("/chat")]
public interface ChatClient
{
    [SocketRequest]
    void Receive(Models.ChatMessage message);
}
