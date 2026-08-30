// Appended after a `yon.java` handler. Everything below is the protocol.
//
// The author writes a class and methods. Reading standard input, dispatching
// on the method and writing the response envelope is Tachyon's half.
//
// Java has no JSON in its standard library, so one is supplied here rather
// than asked for: a handler compiles as a single source file with JEP 330,
// so there is no classpath to put a library on and nothing to import from.
// `Json.parse` and `Json.write` are simply in scope.
//
// Everything is package-private. Java allows one public type per file and
// names the file after it, so five public types would have to be five files.

// Java source-file launch mode executes the first top-level class. Keep this
// launcher first even though the protocol implementation and authored
// controller are declared later in the compilation unit.
final class YonLauncher {
    public static void main(String[] args) throws java.io.IOException {
        Yon.main(args);
    }
}

// The stereotypes, declared here because a single source file has no
// classpath to find them on. SOURCE retention: Tachyon reads them before the
// compiler runs, and nothing reads them back.
@java.lang.annotation.Retention(java.lang.annotation.RetentionPolicy.SOURCE)
@interface Controller {}
@java.lang.annotation.Retention(java.lang.annotation.RetentionPolicy.SOURCE)
@interface Service {}
@java.lang.annotation.Retention(java.lang.annotation.RetentionPolicy.SOURCE)
@interface Repository {}
@java.lang.annotation.Retention(java.lang.annotation.RetentionPolicy.SOURCE)
@interface Client {}
@java.lang.annotation.Retention(java.lang.annotation.RetentionPolicy.SOURCE)
@interface Delegate {}

/** Marks a method as a proxy for a program Yon does not run.
 *
 * RUNTIME retention, unlike the five above: those are read by Tachyon before
 * the compiler runs and nothing reads them back, where this one is read by the
 * dispatch below, which runs after it. Java is one of the four languages here
 * whose annotations cannot do the work themselves — an annotation is inert, so
 * the dispatch checks for it and the annotated body is never called. */
@java.lang.annotation.Retention(java.lang.annotation.RetentionPolicy.RUNTIME)
@java.lang.annotation.Target(java.lang.annotation.ElementType.METHOD)
@interface Relay {
    String[] value();
}

/** JSON, in the box, because Java does not put it there. */
final class Json {
    private final String source;
    private int at;

    private Json(String source) {
        this.source = source;
    }

    /** Reads one JSON document into maps, lists, strings, numbers and booleans. */
    static Object parse(String source) {
        Json reader = new Json(source == null || source.isBlank() ? "{}" : source);
        return reader.value();
    }

    /** Writes one value as JSON. */
    static String write(Object value) {
        if (value == null) return "null";
        if (value instanceof String text) return quote(text);
        if (value instanceof Boolean flag) return flag ? "true" : "false";
        if (value instanceof Number number) return number.toString();
        if (value instanceof java.util.Map<?, ?> map) {
            StringBuilder out = new StringBuilder("{");
            for (java.util.Map.Entry<?, ?> entry : map.entrySet()) {
                if (out.length() > 1) out.append(',');
                out.append(quote(String.valueOf(entry.getKey())))
                   .append(':')
                   .append(write(entry.getValue()));
            }
            return out.append('}').toString();
        }
        if (value instanceof java.util.List<?> list) {
            StringBuilder out = new StringBuilder("[");
            for (Object item : list) {
                if (out.length() > 1) out.append(',');
                out.append(write(item));
            }
            return out.append(']').toString();
        }
        // Anything else crosses as its text, which is what a handler returning
        // an unforeseen type would want to see at the other end.
        return quote(String.valueOf(value));
    }

    static String quote(String text) {
        StringBuilder out = new StringBuilder("\"");
        for (int index = 0; index < text.length(); index += 1) {
            char character = text.charAt(index);
            switch (character) {
                case '"' -> out.append("\\\"");
                case '\\' -> out.append("\\\\");
                case '\n' -> out.append("\\n");
                case '\r' -> out.append("\\r");
                case '\t' -> out.append("\\t");
                default -> {
                    if (character < 0x20) out.append(String.format("\\u%04x", (int) character));
                    else out.append(character);
                }
            }
        }
        return out.append('"').toString();
    }

    private void skip() {
        while (at < source.length() && Character.isWhitespace(source.charAt(at))) at += 1;
    }

    private Object value() {
        skip();
        if (at >= source.length()) return null;
        return switch (source.charAt(at)) {
            case '{' -> object();
            case '[' -> array();
            case '"' -> string();
            case 't' -> { at += 4; yield Boolean.TRUE; }
            case 'f' -> { at += 5; yield Boolean.FALSE; }
            case 'n' -> { at += 4; yield null; }
            default -> number();
        };
    }

    private java.util.Map<String, Object> object() {
        java.util.Map<String, Object> entries = new java.util.LinkedHashMap<>();
        at += 1;
        while (at < source.length()) {
            skip();
            char character = source.charAt(at);
            if (character == '}') { at += 1; break; }
            if (character == ',') { at += 1; continue; }
            String key = string();
            skip();
            if (at < source.length() && source.charAt(at) == ':') at += 1;
            entries.put(key, value());
        }
        return entries;
    }

    private java.util.List<Object> array() {
        java.util.List<Object> values = new java.util.ArrayList<>();
        at += 1;
        while (at < source.length()) {
            skip();
            char character = source.charAt(at);
            if (character == ']') { at += 1; break; }
            if (character == ',') { at += 1; continue; }
            values.add(value());
        }
        return values;
    }

    private String string() {
        StringBuilder out = new StringBuilder();
        if (at >= source.length() || source.charAt(at) != '"') return out.toString();
        at += 1;
        while (at < source.length()) {
            char character = source.charAt(at);
            at += 1;
            if (character == '"') break;
            if (character != '\\') { out.append(character); continue; }
            char escaped = source.charAt(at);
            at += 1;
            switch (escaped) {
                case 'n' -> out.append('\n');
                case 'r' -> out.append('\r');
                case 't' -> out.append('\t');
                case 'b' -> out.append('\b');
                case 'f' -> out.append('\f');
                case 'u' -> {
                    out.append((char) Integer.parseInt(source.substring(at, at + 4), 16));
                    at += 4;
                }
                default -> out.append(escaped);
            }
        }
        return out.toString();
    }

    private Object number() {
        int start = at;
        while (at < source.length() && "0123456789+-.eE".indexOf(source.charAt(at)) >= 0) at += 1;
        String digits = source.substring(start, at);
        // Whole numbers cross as longs, because a handler counting things wrote
        // an integer and expects one back.
        if (digits.matches("-?\\d+")) return Long.valueOf(digits);
        return Double.valueOf(digits);
    }
}

/** One request, as the protocol delivers it. */
final class YonRequest {
    private final java.util.Map<String, Object> raw;

    @SuppressWarnings("unchecked")
    YonRequest(Object parsed) {
        this.raw = parsed instanceof java.util.Map ? (java.util.Map<String, Object>) parsed : java.util.Map.of();
    }

    /** The HTTP method, upper case. */
    public String method() {
        return String.valueOf(raw.getOrDefault("method", ""));
    }

    /** The route as matched, with its parameters still in it. */
    public String route() {
        return String.valueOf(raw.getOrDefault("route", ""));
    }

    /** One bound route parameter, or an empty string when there is none. */
    @SuppressWarnings("unchecked")
    public String parameter(String name) {
        Object parameters = raw.get("parameters");
        if (!(parameters instanceof java.util.Map)) return "";
        Object value = ((java.util.Map<String, Object>) parameters).get(name);
        return value == null ? "" : String.valueOf(value);
    }

    /** The request body as text, empty when none was sent. */
    @SuppressWarnings("unchecked")
    public String body() {
        Object body = raw.get("body");
        if (!(body instanceof java.util.Map)) return "";
        Object data = ((java.util.Map<String, Object>) body).get("data");
        return data == null ? "" : String.valueOf(data);
    }

    /** The whole request, for anything the accessors above do not cover. */
    public java.util.Map<String, Object> raw() {
        return raw;
    }
}

/** One response, built the way the other layers build their return values. */
final class YonResponse {
    private int status;
    private final java.util.Map<String, java.util.List<String>> headers = new java.util.LinkedHashMap<>();
    private String body;

    private YonResponse(int status, String body) {
        this.status = status;
        this.body = body;
    }

    /** A JSON body with a 200. The common case, so it is the short one. */
    public static YonResponse json(String body) {
        YonResponse response = new YonResponse(200, body);
        response.headers.put("content-type", java.util.List.of("application/json"));
        return response;
    }

    /** A response with no body, for a 204 or a redirect. */
    public static YonResponse empty(int status) {
        return new YonResponse(status, "");
    }

    public YonResponse status(int status) {
        this.status = status;
        return this;
    }

    /** Replaces every header at once, for a delegate answering with its own.
     * Without this the assumed content type would survive alongside the one
     * the delegate actually set. */
    YonResponse headers(java.util.Map<String, java.util.List<String>> replacement) {
        headers.clear();
        headers.putAll(replacement);
        return this;
    }

    public YonResponse header(String name, String value) {
        // A header value is a list, because one header may repeat.
        headers.computeIfAbsent(name, key -> new java.util.ArrayList<>()).add(value);
        return this;
    }

    String envelope() {
        java.util.Map<String, Object> envelope = new java.util.LinkedHashMap<>();
        envelope.put("status", status);
        envelope.put("headers", headers);
        envelope.put("body", body);
        return Json.write(envelope);
    }
}

/** The entry point Tachyon supplies, so the handler does not write one.
 *
 * Named `Yon` because JEP 330 finds the entry class by the file's name rather
 * than by its position, so the staged copy is `<digest>/Yon.java`. The digest
 * is the directory: a class name cannot begin with a digit or hold a hyphen. */
final class Yon {
    static final int MAX_RELAY_STDOUT_BYTES = 16 * 1024 * 1024;
    static final int MAX_RELAY_STDERR_BYTES = 64 * 1024;

    static final class RelayOutput {
        final byte[] bytes;
        final boolean overflow;
        RelayOutput(byte[] bytes, boolean overflow) {
            this.bytes = bytes;
            this.overflow = overflow;
        }
    }

    static RelayOutput drain(java.io.InputStream pipe, int limit) {
        var kept = new java.io.ByteArrayOutputStream(Math.min(limit, 8192));
        var chunk = new byte[8192];
        boolean overflow = false;
        try (pipe) {
            while (true) {
                int count = pipe.read(chunk);
                if (count < 0) break;
                int remaining = Math.max(0, limit - kept.size());
                kept.write(chunk, 0, Math.min(count, remaining));
                overflow |= count > remaining;
            }
        } catch (java.io.IOException ignored) {
            overflow = true;
        }
        return new RelayOutput(kept.toByteArray(), overflow);
    }

    static void killTree(Process child) {
        child.descendants().forEach(handle -> handle.destroyForcibly());
        child.destroyForcibly();
    }

    /** Runs a handler written in a language Yon does not run.
     *
     * Yon runs the eight languages that can declare a layer. Go, Ruby, Elixir
     * and the rest cannot, so they are not routes — but they are still
     * programs, and a program that speaks Handler Protocol v1 on standard
     * input and output is exactly what Yon spawns anyway.
     *
     * The command is explicit rather than inferred from the file name: a
     * compiled language has no interpreter to infer. The working directory is
     * the project root, so a project-relative path reads as written. */
    @SuppressWarnings("unchecked")
    static YonResponse relay(java.util.List<String> command, YonRequest request) {
        if (command.isEmpty()) return failed("A delegate command cannot be empty.");
        try {
            // No shell: a route parameter that reached a command line would be
            // an injection.
            Process child = new ProcessBuilder(command).start();
            var stdoutPipe = child.getInputStream();
            var stderrPipe = child.getErrorStream();
            var stdoutTask = java.util.concurrent.CompletableFuture.supplyAsync(
                () -> drain(stdoutPipe, MAX_RELAY_STDOUT_BYTES));
            var stderrTask = java.util.concurrent.CompletableFuture.supplyAsync(
                () -> drain(stderrPipe, MAX_RELAY_STDERR_BYTES));
            try (var stdin = child.getOutputStream()) {
                // Closed before the output is read, so the child sees end of
                // input rather than waiting while this side waits for an answer.
                stdin.write(Json.write(request.raw()).getBytes(java.nio.charset.StandardCharsets.UTF_8));
            }
            Object requested = request.raw().get("deadline_ms");
            long timeout = requested instanceof Number number
                ? Math.max(1, Math.min(300_000, number.longValue()))
                : 30_000;
            if (!child.waitFor(timeout, java.util.concurrent.TimeUnit.MILLISECONDS)) {
                killTree(child);
                child.waitFor(1, java.util.concurrent.TimeUnit.SECONDS);
                return failed("Delegate invocation failed.");
            }
            RelayOutput stdout;
            RelayOutput stderr;
            try {
                stdout = stdoutTask.get(1, java.util.concurrent.TimeUnit.SECONDS);
                stderr = stderrTask.get(1, java.util.concurrent.TimeUnit.SECONDS);
            } catch (java.util.concurrent.TimeoutException unsettled) {
                stdoutPipe.close();
                stderrPipe.close();
                killTree(child);
                return failed("Delegate invocation failed.");
            }
            if (child.exitValue() != 0 || stdout.overflow || stderr.overflow) {
                return failed("Delegate invocation failed.");
            }
            String output = new String(stdout.bytes, java.nio.charset.StandardCharsets.UTF_8);
            if (!(Json.parse(output) instanceof java.util.Map<?, ?> envelope)) {
                return failed("Delegate returned an invalid response.");
            }
            Object body = envelope.get("body");
            YonResponse response = YonResponse.json(body == null ? "" : String.valueOf(body));
            Object code = envelope.get("status");
            if (code instanceof Number number) response = response.status(number.intValue());
            // The headers the delegate set replace the assumed content type,
            // because a delegate that answered with a header meant that header.
            if (envelope.get("headers") instanceof java.util.Map<?, ?> written && !written.isEmpty()) {
                response = response.headers(java.util.Map.of());
                for (java.util.Map.Entry<?, ?> entry : written.entrySet()) {
                    Object values = entry.getValue();
                    for (Object value : values instanceof java.util.List<?> list
                            ? list
                            : java.util.List.of(values)) {
                        response = response.header(
                            String.valueOf(entry.getKey()), String.valueOf(value));
                    }
                }
            }
            return response;
        } catch (InterruptedException interrupted) {
            Thread.currentThread().interrupt();
            return failed("Delegate invocation failed.");
        } catch (java.io.IOException error) {
            return failed("Delegate invocation failed.");
        } catch (java.util.concurrent.ExecutionException error) {
            return failed("Delegate invocation failed.");
        }
    }

    /** A delegate that could not be run answers 502, the same as any other
     * upstream that did not reply. Delegate stderr and process errors are
     * diagnostic-only and never become client response data. */
    private static YonResponse failed(String reason) {
        return YonResponse.json(Json.write(java.util.Map.of("error", reason))).status(502);
    }

    public static void main(String[] args) throws java.io.IOException {
        String raw = read(System.in);
        YonRequest request = new YonRequest(Json.parse(raw));
        YonResponse response;
        try {
            // Found by reflection rather than by a fixed list, so a method the
            // handler does not declare answers 405 without being written.
            java.lang.reflect.Method method = __YON_CONTROLLER__.class
                .getDeclaredMethod(request.method(), YonRequest.class);
            method.setAccessible(true);
            response = (YonResponse) method.invoke(null, request);
        } catch (NoSuchMethodException absent) {
            response = YonResponse.empty(405);
        } catch (java.lang.ReflectiveOperationException failed) {
            Throwable cause = failed.getCause() == null ? failed : failed.getCause();
            response = YonResponse.json(
                Json.write(java.util.Map.of("error", String.valueOf(cause.getMessage())))).status(500);
        }
        System.out.print(response.envelope());
    }

    private static String read(java.io.InputStream stream) throws java.io.IOException {
        return new String(stream.readAllBytes(), java.nio.charset.StandardCharsets.UTF_8);
    }
}
