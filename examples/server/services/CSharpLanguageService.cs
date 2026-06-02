using System.Collections.Generic;
using System.Text.Json;

public class CSharpLanguageService
{
    private readonly CSharpFyloRepository _fyloRepository = new();

    public Dictionary<string, object?> Describe(JsonElement request)
    {
        var requestId = "unknown";
        if (request.TryGetProperty("context", out var context)
            && context.TryGetProperty("requestId", out var requestIdValue))
        {
            requestId = requestIdValue.GetString() ?? "unknown";
        }

        return new Dictionary<string, object?>
        {
            ["language"] = "csharp",
            ["message"] = "Hello from C#!",
            ["requestId"] = requestId,
            ["fylo"] = _fyloRepository.ReadCurrentSchema(),
        };
    }
}
