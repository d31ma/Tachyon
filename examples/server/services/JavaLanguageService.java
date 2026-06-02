import java.util.LinkedHashMap;
import java.util.Map;

public class JavaLanguageService {
    private final JavaFyloRepository fyloRepository = new JavaFyloRepository();

    @SuppressWarnings("unchecked")
    public Object create(Map<String, Object> request) {
        Map<String, Object> context = (Map<String, Object>) request.get("context");
        Object requestId = context == null ? "unknown" : context.get("requestId");
        Map<String, Object> response = new LinkedHashMap<>();
        response.put("message", "Hello from Java!");
        response.put("requestId", requestId);
        response.put("fylo", fyloRepository.latestDocument(String.valueOf(requestId)));
        return response;
    }
}
