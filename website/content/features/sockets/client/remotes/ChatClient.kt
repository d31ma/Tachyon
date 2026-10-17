import models.ChatMessage

@HTTPClient("/chat")
interface ChatClient {
    @SocketRequest
    fun receive(message: ChatMessage)
}
