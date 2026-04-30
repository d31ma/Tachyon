import java.util.Map;

public class POST {
    public static Object handler(Map<String, Object> request) {
        return new JavaLanguageService().create(request);
    }
}
