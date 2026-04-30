using System.Collections.Generic;
using System.Text.Json;

public class GET
{
    private readonly CSharpLanguageService cSharpLanguageService = new CSharpLanguageService();

    public Dictionary<string, object?> Handler(JsonElement request)
    {
        return this.cSharpLanguageService.Describe(request);
    }
}
