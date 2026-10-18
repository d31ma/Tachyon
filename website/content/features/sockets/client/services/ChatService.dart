@Service
class ChatService {
  @Needs
  final ChatClient chatClient;
  List<ChatMessage> messages = [];

  @OnEvent
  void addMessage(ChatMessage message) {
    messages = [...messages.skip(messages.length > 49 ? messages.length - 49 : 0), message];
  }

  Future<void> sayHello() => chatClient.send(const ChatMessage('Hello from a tab'));
}
