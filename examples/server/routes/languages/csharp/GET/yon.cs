using System.Collections.Generic;
using System.Text.Json;

public class Yon
{
    private readonly CSharpLanguageService cSharpLanguageService = new CSharpLanguageService();

    public Dictionary<string, object?> Handler(JsonElement request)
    {
        var responses = new Dictionary<string, Dictionary<string, object?>>
        {
            ["423"] = Response("423", "locked"),
            ["424"] = Response("424", "failed dependency"),
            ["425"] = Response("425", "too early"),
            ["426"] = Response("426", "upgrade required"),
            ["428"] = Response("428", "precondition required"),
        };
        if (request.TryGetProperty("query", out var query)
            && query.TryGetProperty("code", out var raw))
        {
            var code = raw.ValueKind == JsonValueKind.Number
                ? raw.GetInt32().ToString()
                : raw.GetString() ?? "";
            if (responses.TryGetValue(code, out var response))
                return response;
        }

        return this.cSharpLanguageService.Describe(request);
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
