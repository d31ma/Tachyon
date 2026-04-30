import java.util.LinkedHashMap;
import java.util.Map;

public class JavaLanguageService {
    @SuppressWarnings("unchecked")
    public Object create(Map<String, Object> request) {
        Map<String, Object> context = (Map<String, Object>) request.get("context");
        Map<String, Object> response = new LinkedHashMap<>();
        response.put("message", "Hello from Java!");
        response.put("requestId", context == null ? "unknown" : context.get("requestId"));
        return response;
    }
}
