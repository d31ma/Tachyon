import java.io.BufferedReader;
import java.io.OutputStreamWriter;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

public class JavaFyloRepository {
    private final String root = System.getenv().getOrDefault("FYLO_ROOT", System.getProperty("user.dir") + "/db");
    private final String executable = System.getenv("FYLO_EXEC_PATH");

    public Map<String, Object> latestDocument(String requestId) {
        String collection = "language-route-events";
        machine(Map.of("op", "createCollection", "collection", collection));
        Object id = machine(Map.of(
            "op", "putData",
            "collection", collection,
            "data", Map.of("language", "java", "source", "fylo.exec", "requestId", requestId)
        ));
        machine(Map.of("op", "getLatest", "collection", collection, "id", String.valueOf(id)));
        return summary(collection, List.of("createCollection", "putData", "getLatest"), 3);
    }

    private Object machine(Map<String, ?> request) {
        try {
            List<String> command = executable == null || executable.isBlank()
                ? List.of("bunx", "--bun", "fylo.exec", "exec", "--request", "-", "--root", root)
                : List.of(executable, "exec", "--request", "-", "--root", root);
            Process process = new ProcessBuilder(command).start();
            try (OutputStreamWriter stdin = new OutputStreamWriter(process.getOutputStream(), StandardCharsets.UTF_8)) {
                stdin.write(YonJson.stringify(request));
            }
            String stdout = new BufferedReader(new InputStreamReader(process.getInputStream(), StandardCharsets.UTF_8)).lines().reduce("", (a, b) -> a + b);
            String stderr = new BufferedReader(new InputStreamReader(process.getErrorStream(), StandardCharsets.UTF_8)).lines().reduce("", (a, b) -> a + b);
            int code = process.waitFor();
            if (code != 0) throw new RuntimeException(stderr.isBlank() ? stdout : stderr);
            Map<String, Object> response = YonJson.parseObject(stdout.isBlank() ? "{}" : stdout);
            if (!Boolean.TRUE.equals(response.get("ok"))) throw new RuntimeException("fylo.exec returned an error");
            return response.get("result");
        } catch (Exception error) {
            throw new RuntimeException(error);
        }
    }

    private Map<String, Object> summary(String collection, List<String> operations, int count) {
        Map<String, Object> out = new LinkedHashMap<>();
        out.put("collection", collection);
        out.put("operations", operations);
        out.put("resultCount", String.valueOf(count));
        return out;
    }
}
