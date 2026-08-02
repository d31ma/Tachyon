// Appended to a tac.cs companion. The author writes plain C# in a static class
// named Companion and declares which members the island may reach; everything
// below is the ABI of ADR 0011.
//
// The JSON is built by hand rather than serialised: JsonSerializer needs
// reflection over the companion's types, which is exactly what a trimmed wasm
// publish removes. Parsing goes through JsonNode, which does not.

/// <summary>One member the island may reach, declared in Companion.Tac.</summary>
public sealed class TacMember
{
    public Func<object> Read;
    public Action<object> Write;
    public Func<object[], object> Invoke;

    public static TacMember Field(Func<object> read, Action<object> write = null) =>
        new TacMember { Read = read, Write = write };

    public static TacMember Method(Func<object[], object> invoke) =>
        new TacMember { Invoke = invoke };
}

public partial class Tac
{
    private static string Encode(string text)
    {
        var encoded = new StringBuilder("\"");
        foreach (var character in text)
        {
            switch (character)
            {
                case '"': encoded.Append("\\\""); break;
                case '\\': encoded.Append("\\\\"); break;
                case '\n': encoded.Append("\\n"); break;
                case '\t': encoded.Append("\\t"); break;
                case '\r': encoded.Append("\\r"); break;
                default:
                    if (character < 0x20) encoded.Append("\\u").Append(((int)character).ToString("x4"));
                    else encoded.Append(character);
                    break;
            }
        }
        return encoded.Append('"').ToString();
    }

    private static string Encode(object value)
    {
        switch (value)
        {
            case null: return "null";
            case bool flag: return flag ? "true" : "false";
            case string text: return Encode(text);
            case int number: return number.ToString(CultureInfo.InvariantCulture);
            case long number: return number.ToString(CultureInfo.InvariantCulture);
            case double number: return number.ToString("R", CultureInfo.InvariantCulture);
            case float number: return number.ToString("R", CultureInfo.InvariantCulture);
            case decimal number: return number.ToString(CultureInfo.InvariantCulture);
            case IEnumerable values when value is not string:
                var items = new List<string>();
                foreach (var item in values) items.Add(Encode(item));
                return "[" + string.Join(",", items) + "]";
            // Anything else crosses as its text, which is what a companion
            // returning an unforeseen type would want to see in the page.
            default: return Encode(value.ToString());
        }
    }

    /// <summary>The C# value an author's closure expects, from one JSON node.</summary>
    private static object Value(JsonNode node)
    {
        if (node is not JsonValue value) return node?.ToJsonString();
        if (value.TryGetValue<bool>(out var flag)) return flag;
        if (value.TryGetValue<int>(out var whole)) return whole;
        if (value.TryGetValue<double>(out var number)) return number;
        if (value.TryGetValue<string>(out var text)) return text;
        return null;
    }

    [JSExport]
    internal static string Invoke(string request)
    {
        try
        {
            var parsed = JsonNode.Parse(request)?.AsObject();
            var operation = parsed?["op"]?.GetValue<string>();
            if (operation == "init")
            {
                var fields = new List<string>();
                var methods = new List<string>();
                foreach (var member in Companion.Tac)
                {
                    if (member.Value.Read != null) fields.Add(member.Key);
                    else methods.Add(member.Key);
                }
                return "{\"value\":{\"fields\":" + Encode(fields) + ",\"methods\":"
                    + Encode(methods) + "}}";
            }
            var name = parsed?["name"]?.GetValue<string>();
            if (name == null || !Companion.Tac.TryGetValue(name, out var found))
                return "{\"error\":\"Unknown companion member.\"}";
            switch (operation)
            {
                case "get" when found.Read != null:
                    return "{\"value\":" + Encode(found.Read()) + "}";
                case "set" when found.Write != null:
                    found.Write(Value(parsed["value"]));
                    return "{\"value\":null}";
                case "call" when found.Invoke != null:
                    var arguments = new List<object>();
                    if (parsed["args"] is JsonArray given)
                        foreach (var argument in given) arguments.Add(Value(argument));
                    return "{\"value\":" + Encode(found.Invoke(arguments.ToArray())) + "}";
                default:
                    return "{\"error\":\"Companion member does not support " + operation + ".\"}";
            }
        }
        catch (Exception error)
        {
            return "{\"error\":" + Encode(error.Message) + "}";
        }
    }
}

// A wasm publish needs an entry point even though the island calls in through
// the export above and never runs it.
public class TacProgram
{
    public static void Main() { }
}
