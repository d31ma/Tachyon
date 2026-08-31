// Appended to a tac.cs companion compiled into a Windows host.
//
// The WebAssembly prelude beside this one reaches the page through [JSExport],
// which needs the browser runtime; this one is published by NativeAOT as a
// plain DLL the Win32 host loads, so the export is an ordinary C entry point.
// Everything between the two is the same, because the protocol is.
//
// The JSON is built by hand rather than serialised: JsonSerializer needs
// reflection over the companion's types, which is exactly what an AOT publish
// removes. Parsing goes through JsonNode, which does not.

/// <summary>One member the host may reach, declared in TacBridge.Tac.</summary>

// The browser's two lifetimes, on Windows. What survives every launch goes to
// a file under LocalApplicationData — the same place ApplicationData.
// LocalSettings writes for a packaged app, reachable from an unpackaged one.
// A session is this process, because a native app has no tabs.
public static class TacStore
{
    private static readonly Dictionary<string, object> SessionValues = new();
    private static readonly Dictionary<string, string> LocalValues = Load();
    private static string Path_ => System.IO.Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "Tachyon", TacApplication.Id, "store.txt");

    private static Dictionary<string, string> Load()
    {
        var values = new Dictionary<string, string>();
        try
        {
            foreach (var line in System.IO.File.ReadAllLines(Path_))
            {
                var split = line.IndexOf('\t');
                if (split > 0) values[line[..split]] = line[(split + 1)..];
            }
        }
        catch (Exception) { /* No store yet is not an error. */ }
        return values;
    }

    public static string Local(string key, string fallback) =>
        LocalValues.TryGetValue(key, out var value) ? value : fallback;

    public static void SetLocal(string key, string value)
    {
        LocalValues[key] = value;
        try
        {
            System.IO.Directory.CreateDirectory(System.IO.Path.GetDirectoryName(Path_)!);
            System.IO.File.WriteAllLines(Path_, LocalValues.Select(pair => pair.Key + "\t" + pair.Value));
        }
        catch (Exception) { /* An unwritable store loses the value, not the app. */ }
    }

    public static object Session(string key, object fallback) =>
        SessionValues.TryGetValue(key, out var value) ? value : fallback;

    public static void SetSession(string key, object value) => SessionValues[key] = value;
}

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
            // returning an unforeseen type would want to see on screen.
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

    // Publishing to the page.
    //
    // Everything else here is the page asking a question. This is the other
    // direction, and the reason it exists: a companion watching something the
    // platform tells it about — a power state, a device change, a file — has
    // no question to answer, because nobody asked one.
    //
    // A raw function pointer rather than a delegate, for the same reason the
    // JSON is built by hand: marshalling a delegate needs the reflection an
    // AOT publish removes.
    private static unsafe delegate* unmanaged<IntPtr, void> Emit;

    internal static unsafe void SetEmit(IntPtr emit) =>
        Emit = (delegate* unmanaged<IntPtr, void>)emit;

    /// <summary>
    /// Publishes a value to the page, where <c>@subscribe(name)</c> receives
    /// it. A no-op until the host installs its sink.
    /// </summary>
    public static unsafe void Publish(string name, object value = null)
    {
        if (Emit == null) return;
        var payload = Marshal.StringToCoTaskMemUTF8(
            "{\"name\":" + Encode(name) + ",\"value\":" + Encode(value) + "}");
        // Freed here rather than by the host: this side allocated it, and the
        // host has copied whatever it needed before returning.
        try { Emit(payload); }
        finally { Marshal.FreeCoTaskMem(payload); }
    }

    public static string Invoke(string request)
    {
        try
        {
            var parsed = JsonNode.Parse(request)?.AsObject();
            var members = TacRoutes.Members(parsed?["route"]?.GetValue<string>() ?? "");
            if (members == null)
                if (parsed?["op"]?.GetValue<string>() == "init")
                    return "{\"value\":{\"fields\":[],\"methods\":[]}}";
                else
                return "{\"error\":\"Unknown companion route.\",\"code\":\"TY_NATIVE_ROUTE\"}";
            var operation = parsed?["op"]?.GetValue<string>();
            if (operation == "init")
            {
                var fields = new List<string>();
                var methods = new List<string>();
                foreach (var member in members)
                {
                    if (member.Value.Read != null) fields.Add(member.Key);
                    else methods.Add(member.Key);
                }
                return "{\"value\":{\"fields\":" + Encode(fields) + ",\"methods\":"
                    + Encode(methods) + "}}";
            }
            var name = parsed?["name"]?.GetValue<string>();
            if (name == null || !members.TryGetValue(name, out var found))
                return "{\"error\":\"Unknown companion member.\"}";
            switch (operation)
            {
                case "get" when found.Read != null:
                    return "{\"value\":" + Encode(found.Read()) + "}";
                case "set" when found.Write != null:
                    found.Write(Value(parsed["value"]));
                    return "{\"value\":null}";
                case "set" when found.Read != null:
                    return "{\"error\":\"Companion field is read-only: " + name + ".\"}";
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

// The host's view of the companion: two ordinary C functions, because that is
// all a Win32 host in C can call. The answer is allocated here and freed here,
// so neither side has to know how the other's allocator works.
public static class TacNative
{
    [UnmanagedCallersOnly(EntryPoint = "tac_native_invoke")]
    public static IntPtr NativeInvoke(IntPtr request)
    {
        var text = Marshal.PtrToStringUTF8(request);
        return Marshal.StringToCoTaskMemUTF8(Tac.Invoke(text ?? "{}"));
    }

    [UnmanagedCallersOnly(EntryPoint = "tac_native_free")]
    public static void NativeFree(IntPtr answer) => Marshal.FreeCoTaskMem(answer);

    /// <summary>
    /// Installs the host's sink for <see cref="Tac.Publish"/>. A host that
    /// never calls this gets a companion that publishes into nothing, which
    /// is what an older host loading a newer companion should do.
    /// </summary>
    [UnmanagedCallersOnly(EntryPoint = "tac_native_set_emit")]
    public static unsafe void NativeSetEmit(IntPtr emit) => Tac.SetEmit(emit);
}
