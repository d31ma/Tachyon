import java.util.LinkedHashMap;
import java.util.Map;

public class Yon {
    public static Object handler(Map<String, Object> request) {
        Map<String, Map<String, Object>> responses = new LinkedHashMap<>();
        responses.put("416", response("416", "range not satisfiable"));
        responses.put("417", response("417", "expectation failed"));
        responses.put("418", response("418", "teapot"));
        responses.put("421", response("421", "misdirected"));
        responses.put("422", response("422", "unprocessable"));
        Object raw = request.containsKey("query") && request.get("query") instanceof Map
            ? ((Map<?, ?>) request.get("query")).get("code")
            : "";
        String code = raw instanceof Number ? String.valueOf(((Number) raw).intValue()) : String.valueOf(raw == null ? "" : raw);
        if (responses.containsKey(code)) {
            return responses.get(code);
        }

        return new JavaLanguageService().create(request);
    }

    private static Map<String, Object> response(String code, String detail) {
        Map<String, Object> body = new LinkedHashMap<>();
        body.put("code", code);
        body.put("detail", detail);
        return body;
    }
}
