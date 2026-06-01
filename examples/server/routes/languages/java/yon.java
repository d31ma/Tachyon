import java.util.LinkedHashMap;
import java.util.Map;

public class Handler {
    private static final JavaLanguageService SERVICE = new JavaLanguageService();
    private static final Map<String, Map<String, Object>> RESPONSES = responses();

    public static Object POST(Map<String, Object> request) {
        String code = statusCode(request);
        if (RESPONSES.containsKey(code)) {
            return RESPONSES.get(code);
        }

        return SERVICE.create(request);
    }

    @SuppressWarnings("unchecked")
    private static String statusCode(Map<String, Object> request) {
        Object raw = request.containsKey("query") && request.get("query") instanceof Map
            ? ((Map<String, Object>) request.get("query")).get("code")
            : "";
        return raw instanceof Number ? String.valueOf(((Number) raw).intValue()) : String.valueOf(raw == null ? "" : raw);
    }

    private static Map<String, Map<String, Object>> responses() {
        Map<String, Map<String, Object>> responses = new LinkedHashMap<>();
        responses.put("416", response("416", "range not satisfiable"));
        responses.put("417", response("417", "expectation failed"));
        responses.put("418", response("418", "teapot"));
        responses.put("421", response("421", "misdirected"));
        responses.put("422", response("422", "unprocessable"));
        return responses;
    }

    private static Map<String, Object> response(String code, String detail) {
        Map<String, Object> body = new LinkedHashMap<>();
        body.put("code", code);
        body.put("detail", detail);
        return body;
    }
}
