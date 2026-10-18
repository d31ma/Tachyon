internal static class BehaviorBrowserSocket
{
    internal static System.Func<string, object, System.Threading.Tasks.Task<string>> Host = null;
    private static readonly string[] KnownCodes = { "HTTP_MISSING", "HTTP_REQUEST", "HTTP_BODY", "CANCELLED" };

    internal static async System.Threading.Tasks.Task Send(string client, object message)
    {
        if (Behaviors.Environment != "tac" || Host == null) throw new BehaviorError("HTTP_MISSING", null, "HTTP_MISSING: The socket message was not sent.");
        var code = Failure(await Host(client, message));
        if (code != null) throw new BehaviorError(code, null, code + ": The socket message was not sent.");
    }

    private static string Failure(string answer)
    {
        try
        {
            using var parsed = System.Text.Json.JsonDocument.Parse(answer);
            var root = parsed.RootElement;
            if (root.ValueKind == System.Text.Json.JsonValueKind.Object && root.TryGetProperty("value", out _)) return null;
            var written = root.ValueKind == System.Text.Json.JsonValueKind.Object && root.TryGetProperty("error", out var error)
                && error.ValueKind == System.Text.Json.JsonValueKind.Object && error.TryGetProperty("code", out var code)
                && code.ValueKind == System.Text.Json.JsonValueKind.String ? code.GetString() : null;
            return System.Array.IndexOf(KnownCodes, written) >= 0 ? written : "HTTP_NETWORK";
        }
        catch (System.Text.Json.JsonException) { return "HTTP_NETWORK"; }
    }
}
