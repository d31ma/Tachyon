using System.Collections.Generic;
using System.Text.Json;

public class Handler
{
    private static readonly CSharpLanguageService Service = new();

    private static readonly Dictionary<string, Dictionary<string, object?>> Responses = new()
    {
        ["423"] = Response("423", "locked"),
        ["424"] = Response("424", "failed dependency"),
        ["425"] = Response("425", "too early"),
        ["426"] = Response("426", "upgrade required"),
        ["428"] = Response("428", "precondition required"),
    };

    public static Dictionary<string, object?> GET(JsonElement request)
    {
        var code = StatusCode(request);
        if (Responses.TryGetValue(code, out var response))
            return response;

        return Service.Describe(request);
    }

    private static string StatusCode(JsonElement request)
    {
        if (!request.TryGetProperty("query", out var query)
            || !query.TryGetProperty("code", out var raw))
            return "";

        return raw.ValueKind == JsonValueKind.Number
            ? raw.GetInt32().ToString()
            : raw.GetString() ?? "";
    }

    private static Dictionary<string, object?> Response(string code, string detail)
    {
        return new Dictionary<string, object?>
        {
            ["code"] = code,
            ["detail"] = detail,
        };
    }
}
