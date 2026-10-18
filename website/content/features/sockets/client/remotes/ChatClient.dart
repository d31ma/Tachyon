@HTTPClient("/chat")
abstract interface class ChatClient {
  @SocketRequest
  void receive(ChatMessage message);
}
