import models.ChatMessage

@Service
class ChatService {
    @Needs
    private val chatClient: ChatClient
    var messages: List<ChatMessage> = emptyList()

    @OnEvent
    fun addMessage(message: ChatMessage) {
        messages = (messages + message).takeLast(messageLimit())
    }

    suspend fun sayHello() {
        chatClient.send(ChatMessage("Hello from a tab"))
    }

    private fun messageLimit(): Int = 50
}
