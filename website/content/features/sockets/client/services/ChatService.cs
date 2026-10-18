[Service]
public sealed class ChatService
{
    private const int MessageLimit = 50;

    [Needs]
    private readonly ChatClient _chatClient;

    public List<Models.ChatMessage> Messages = new();

    [OnEvent]
    public void AddMessage(Models.ChatMessage message)
    {
        Messages.Add(message);
        if (Messages.Count > MessageLimit) Messages.RemoveRange(0, Messages.Count - MessageLimit);
    }

    public System.Threading.Tasks.Task SayHello() => _chatClient.Send(new Models.ChatMessage("Hello from a tab"));
}
