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
        catch (Exception) {}
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
        catch (Exception) {}
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
    public Func<object[], System.Threading.Tasks.Task<object>> InvokeAsync;
    public string[] Events;
    public bool Replay;
    public int Order;
    public JsonNode Condition;
    public JsonNode Models;
    public string Returns;
    public bool Asynchronous;
    public Func<object, object> Listen;
    public bool Destroy;
    public Action Lifecycle;

    public static TacMember Field(Func<object> read, Action<object> write = null) =>
        new TacMember { Read = read, Write = write };

    public static TacMember Method(Func<object[], object> invoke) =>
        new TacMember { Invoke = invoke };

    public static TacMember TaskMethod(Func<object[], System.Threading.Tasks.Task<object>> invoke) =>
        new TacMember { InvokeAsync = invoke };

    public static object Result(Func<object> call) => call();

    public static object Result(Action call)
    {
        call();
        return null;
    }
    public static TacMember Listener(string[] events, bool replay, int order, string condition, string models, string returns, bool asynchronous, Func<object, object> invoke) =>
        new() { Events = events, Replay = replay, Order = order, Condition = Parse(condition), Models = Parse(models), Returns = returns, Asynchronous = asynchronous, Listen = invoke };

    private static JsonNode Parse(string text)
    {
        try { return text == null ? null : JsonNode.Parse(text); }
        catch (Exception) { return null; }
    }
    public static TacMember Hook(bool destroy, Action invoke) => new() { Destroy = destroy, Lifecycle = invoke };
}

public sealed class ApplicationEventPublisher
{
    public void PublishEvent(object value, bool retain = false) => Tac.PublishModel(value, retain);

    public void publishEvent(object value, bool retain = false) => PublishEvent(value, retain);
}

public static partial class TacModels
{
    public static T Read<T>(object value)
    {
        if (typeof(T) == typeof(string) || typeof(T).IsPrimitive) return (T)Convert.ChangeType(value, typeof(T), CultureInfo.InvariantCulture);
        var json = Tac.EncodeValue(value);
        return (T)System.Text.Json.JsonSerializer.Deserialize(json, typeof(T), TacModelContext.Default);
    }

    public static object Prop(string name) => Tac.CreatingProp(name);

    public static List<T> ReadList<T>(object value) =>
        value is IEnumerable items && value is not string ? items.Cast<object>().Select(Read<T>).ToList() : new List<T>();

    public static object Plain(object value)
    {
        if (value == null || value is string || value is bool || value.GetType().IsPrimitive || value is decimal) return value;
        if (value is IDictionary || value is object[]) return value;
        if (value is IEnumerable items) return items.Cast<object>().Select(Plain).ToArray();
        var model = TacModelContext.Default.GetTypeInfo(value.GetType());
        if (model == null) return value;
        return Tac.ParseValue(System.Text.Json.JsonSerializer.Serialize(value, model));
    }


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
            case IDictionary dictionary:
                var entries = new List<string>();
                foreach (DictionaryEntry entry in dictionary) entries.Add(Encode((string)entry.Key) + ":" + Encode(entry.Value));
                return "{" + string.Join(",", entries) + "}";
            case IEnumerable values when value is not string:
                var items = new List<string>();
                foreach (var item in values) items.Add(Encode(item));
                return "[" + string.Join(",", items) + "]";
            default: return Encode(value.ToString());
        }
    }

    private static object Value(JsonNode node)
    {
        if (node is JsonObject map) return map.ToDictionary(pair => pair.Key, pair => Value(pair.Value));
        if (node is JsonArray list) return list.Select(Value).ToArray();
        if (node is not JsonValue value) return node?.ToJsonString();
        if (value.TryGetValue<bool>(out var flag)) return flag;
        if (value.TryGetValue<int>(out var whole)) return whole;
        if (value.TryGetValue<double>(out var number)) return number;
        if (value.TryGetValue<string>(out var text)) return text;
        return null;
    }

    private static unsafe delegate* unmanaged<IntPtr, void> Emit;

    internal static unsafe void SetEmit(IntPtr emit) =>
        Emit = (delegate* unmanaged<IntPtr, void>)emit;

    public static Action<string> WebEmit;

    private static unsafe void SendEvent(string route, string service, string owner, string name, string json, bool retain)
    {
        var text = "{\"route\":" + Encode(route) + ",\"service\":" + Encode(service) + ",\"owner\":" + Encode(owner) + ",\"name\":" + Encode(name) + ",\"value\":" + json + ",\"retain\":" + (retain ? "true" : "false") + "}";
        if (WebEmit != null) { WebEmit(text); return; }
        if (Emit == null) return;
        var payload = Marshal.StringToCoTaskMemUTF8(text);
        try { Emit(payload); }
        finally { Marshal.FreeCoTaskMem(payload); }
    }

    public static string Invoke(string request)
    {
        lock (OwnerLock) {
            if (Handling) return "{\"error\":\"Native owner is busy.\"}";
            Handling = true; OwnerThread = Environment.CurrentManagedThreadId;
            try { return Handle(request); }
            finally { if (Executing != null) Drain(Executing); Executing = null; Handling = false; }
        }
    }

    private sealed class Activation(string route, string service, string owner, Dictionary<string, TacMember> members, System.Threading.CancellationTokenSource cancellation)
    {
        internal readonly string Route = route;
        internal readonly string Service = service;
        internal readonly string Owner = owner;
        internal readonly Dictionary<string, TacMember> Members = members;
        internal bool Disposed;
        internal int PendingTasks;
        internal readonly System.Threading.CancellationTokenSource Cancellation = cancellation;
        internal readonly System.Threading.CancellationToken HttpLifetime = cancellation.Token;
        internal readonly Queue<(string Name, string Json, bool Retain)> Queue = new();
        internal readonly Dictionary<string, string> Retained = new();
        internal readonly Queue<(TacMember Listener, string Json)> Deferred = new();
        internal readonly List<TacMember> Listeners = members
            .OrderBy(pair => pair.Key, StringComparer.Ordinal)
            .Select(pair => pair.Value)
            .Where(member => member.Listen != null)
            .OrderBy(member => member.Order)
            .ToList();
    }
    private static readonly object OwnerLock = new();
    private static readonly Dictionary<string, Activation> Activations = new();
    private static Activation Executing;
    private static System.Threading.CancellationTokenSource CreatingCancellation;
    private static readonly System.Threading.AsyncLocal<Activation> TaskOwner = new();
    private static bool Handling;
    private static int OwnerThread;
    private static void EventFailure() => Console.Error.WriteLine("Tachyon native event delivery failed.");
    internal static void EventPublish(string name, object value, bool retain)
    {
        lock (OwnerLock) { PublishOwnedEvent(name, value, retain); }
    }
    private static Activation EventOwner() => Handling ? Executing : TaskOwner.Value ?? Executing;
    private static bool EventThread() => TaskOwner.Value != null || Environment.CurrentManagedThreadId == OwnerThread;
    private static void PublishOwnedEvent(string name, object value, bool retain)
    {
        var owner = EventOwner();
        if (!EventThread() || owner == null || !Activations.TryGetValue(owner.Owner, out var live) || live != owner || owner.Disposed || owner.Queue.Count >= 128) { EventFailure(); return; }
        try {
            ValidateEvent(value, 0);
            var json = Encode(value);
            if (Encoding.UTF8.GetByteCount(json) > 65536) throw new ArgumentException();
            _ = JsonNode.Parse(json);
            owner.Queue.Enqueue((name, json, retain));
        } catch (Exception) { EventFailure(); }
    }
    private static void ValidateEvent(object value, int depth)
    {
        if (depth > 64) throw new ArgumentException();
        switch (value) {
            case null: case string: case bool: case int: case long: case short: case byte: case decimal: return;
            case double number when double.IsFinite(number): return;
            case float number when float.IsFinite(number): return;
            case IDictionary map:
                foreach (DictionaryEntry entry in map) { if (entry.Key is not string) throw new ArgumentException(); ValidateEvent(entry.Value, depth + 1); }
                return;
            case IEnumerable list:
                foreach (var item in list) ValidateEvent(item, depth + 1);
                return;
            default: throw new ArgumentException();
        }
    }

    private static readonly object Absent = new();

    private static object Operand(JsonNode node, object value, string name)
    {
        var kind = node?["k"]?.GetValue<string>();
        if (kind == "lit") return Value(node["v"]);
        if (kind == "event") return name;
        var current = value;
        foreach (var segment in node?["path"] as JsonArray ?? new JsonArray()) {
            var key = Value(segment);
            if (current is IDictionary<string, object> map) {
                var text = key as string ?? (key is null ? null : Convert.ToString(key, CultureInfo.InvariantCulture));
                current = text != null && map.TryGetValue(text, out var held) ? held : Absent;
            } else if (current is object[] list) {
                var index = key switch {
                    int whole => whole,
                    double number when number == Math.Floor(number) => (int)number,
                    string text when int.TryParse(text, out var parsed) => parsed,
                    _ => -1,
                };
                current = index >= 0 && index < list.Length ? list[index] : Absent;
            } else return Absent;
            if (ReferenceEquals(current, Absent)) return Absent;
        }
        return current;
    }

    private static double? Number(object value) => value switch {
        bool => null,
        int whole => whole,
        long whole => whole,
        double number => number,
        float number => number,
        decimal number => (double)number,
        _ => null,
    };

    private static bool Truthy(object value)
    {
        if (value is null || ReferenceEquals(value, Absent)) return false;
        if (value is bool flag) return flag;
        if (value is string text) return text.Length > 0;
        var number = Number(value);
        return number is null || (number != 0 && !double.IsNaN(number.Value));
    }

    private static bool Identical(object left, object right)
    {
        if (ReferenceEquals(left, Absent) || ReferenceEquals(right, Absent))
            return ReferenceEquals(left, Absent) && ReferenceEquals(right, Absent);
        if (left is null || right is null) return left is null && right is null;
        if (left is bool || right is bool) return left is bool leftFlag && right is bool rightFlag && leftFlag == rightFlag;
        if (left is string || right is string) return left is string leftText && right is string rightText && leftText == rightText;
        var leftNumber = Number(left);
        var rightNumber = Number(right);
        if (leftNumber != null || rightNumber != null) return leftNumber != null && leftNumber == rightNumber;
        if (left is object[] leftList && right is object[] rightList) {
            if (leftList.Length != rightList.Length) return false;
            for (var index = 0; index < leftList.Length; index++)
                if (!Identical(leftList[index], rightList[index])) return false;
            return true;
        }
        if (left is IDictionary<string, object> leftMap && right is IDictionary<string, object> rightMap) {
            if (leftMap.Count != rightMap.Count) return false;
            foreach (var entry in leftMap)
                if (!rightMap.TryGetValue(entry.Key, out var held) || !Identical(entry.Value, held)) return false;
            return true;
        }
        return false;
    }

    private static bool Orders(string operation, object left, object right)
    {
        int comparison;
        var leftNumber = Number(left);
        var rightNumber = Number(right);
        if (leftNumber != null && rightNumber != null) comparison = leftNumber.Value.CompareTo(rightNumber.Value);
        else if (left is string leftText && right is string rightText) comparison = string.CompareOrdinal(leftText, rightText);
        else return false;
        return operation switch {
            "lt" => comparison < 0,
            "le" => comparison <= 0,
            "gt" => comparison > 0,
            _ => comparison >= 0,
        };
    }

    private static bool Matches(JsonNode matcher, object value, string name)
    {
        var kind = matcher?["k"]?.GetValue<string>();
        switch (kind) {
            case "and": return Matches(matcher["l"], value, name) && Matches(matcher["r"], value, name);
            case "or": return Matches(matcher["l"], value, name) || Matches(matcher["r"], value, name);
            case "not": return !Matches(matcher["e"], value, name);
            case "truthy": return Truthy(Operand(matcher["e"], value, name));
            case "cmp":
                var left = Operand(matcher["l"], value, name);
                var right = Operand(matcher["r"], value, name);
                var operation = matcher["op"]?.GetValue<string>();
                if (operation == "eq") return Identical(left, right);
                if (operation == "ne") return !Identical(left, right);
                return Orders(operation, left, right);
            default: return false;
        }
    }

    internal static string EncodeValue(object value) => Encode(value);

    private static JsonObject CreatingProps;
    internal static object CreatingProp(string name) => CreatingProps?[name] is JsonNode given ? Value(given) : null;

    internal static void PublishModel(object value, bool retain)
    {
        bool plainValue = value == null || value is string || value is IEnumerable || value.GetType().IsPrimitive;
        var plain = plainValue ? null : TacModels.Plain(value);
        if (plain is not IDictionary<string, object>) { EventFailure(); return; }
        EventPublish(value.GetType().Name, plain, retain);
    }

    internal static object ParseValue(string json) => Value(JsonNode.Parse(json));
    internal static object PlainJson(object json) => json == null ? null : ParseValue(json.ToString());

    private static void Republish(object result, string returns)
    {
        if (returns == null || result == null) return;
        var items = result is IEnumerable list && result is not string && result is not IDictionary ? list.Cast<object>().ToArray() : new[] { result };
        foreach (var item in items.Take(32)) {
            if (item != null) EventPublish(returns, TacModels.Plain(item), false);
        }
    }

    private static string DescribeListener(string key, TacMember listener) =>
        "{\"key\":" + Encode(key)
        + ",\"events\":" + Encode(listener.Events.ToList())
        + ",\"replay\":" + Encode(listener.Replay)
        + ",\"order\":" + Encode(listener.Order)
        + ",\"asynchronous\":" + Encode(listener.Asynchronous)
        + ",\"models\":" + (listener.Models?.ToJsonString() ?? "null")
        + ",\"condition\":" + (listener.Condition?.ToJsonString() ?? "null") + "}";

    private static void Deliver(TacMember listener, string json, bool republish)
    {
        try {
            var result = listener.Listen(Value(JsonNode.Parse(json)));
            if (republish) Republish(result, listener.Returns);
        } catch (Exception) { EventFailure(); }
    }

    private static string ReceiveEvent(TacMember listener, JsonNode value)
    {
        try {
            var result = listener.Listen(Value(value));
            if (!listener.Asynchronous) Republish(result, listener.Returns);
            return "{\"value\":null}";
        } catch (Exception) {
            return "{\"error\":\"Native event listener failed.\"}";
        }
    }

    private static void Drain(Activation owner)
    {
        var delivered = 0;
        while (!owner.Disposed) {
            if (owner.Deferred.Count > 0) {
                var pending = owner.Deferred.ToArray();
                owner.Deferred.Clear();
                foreach (var item in pending) Deliver(item.Listener, item.Json, false);
                continue;
            }
            if (owner.Queue.Count == 0) return;
            if (delivered++ == 128) { owner.Queue.Clear(); EventFailure(); return; }
            var next = owner.Queue.Dequeue();
            if (WebEmit != null) {
                SendEvent(owner.Route, owner.Service, owner.Owner, next.Name, next.Json, next.Retain);
                continue;
            }
            if (next.Retain) { if (!owner.Retained.ContainsKey(next.Name) && owner.Retained.Count == 32) { EventFailure(); continue; } owner.Retained[next.Name] = next.Json; }
            object value = null;
            try { value = Value(JsonNode.Parse(next.Json)); } catch (Exception) { value = null; }
            foreach (var listener in owner.Listeners) {
                if (Array.IndexOf(listener.Events, next.Name) < 0) continue;
                if (listener.Models != null && !TacModels.Fits(listener.Models, next.Name, value)) { EventFailure(); continue; }
                if (listener.Condition != null && !Matches(listener.Condition, value, next.Name)) continue;
                if (listener.Asynchronous) {
                    if (owner.Deferred.Count >= 128) EventFailure();
                    else owner.Deferred.Enqueue((listener, next.Json));
                    continue;
                }
                Deliver(listener, next.Json, true);
            }
            SendEvent(owner.Route, owner.Service, owner.Owner, next.Name, next.Json, next.Retain);
        }
    }
    private static void DisposeActivation(Activation owner)
    {
        owner.Disposed = true;
        owner.Cancellation.Cancel();
        owner.Cancellation.Dispose();
        NativeCapabilities.Dispose(owner.Owner);
        owner.Queue.Clear(); owner.Retained.Clear(); owner.Deferred.Clear();
        foreach (var member in owner.Members.OrderBy(pair => pair.Key, StringComparer.Ordinal).Select(pair => pair.Value)) {
            if (member.Lifecycle != null && member.Destroy) { try { member.Lifecycle(); } catch (Exception) { EventFailure(); } }
        }
    }
    private static void DisposeOwner(string id)
    {
        if (!Activations.Remove(id, out var owner)) return;
        if (Executing == owner) Executing = null;
        DisposeActivation(owner);
    }
    private static void DisposeAll()
    {
        var owners = Activations.Values.ToList();
        Activations.Clear(); Executing = null;
        foreach (var owner in owners) DisposeActivation(owner);
    }
    private const int StartUpRequestLimit = 1048576;
    private const int MemberRequestLimit = 65536;
    private static string TooLarge(string subject, int size, int limit) =>
        "{\"error\":" + Encode($"The {subject} is {size} bytes. The limit is {limit} bytes. Pass less data to the service.") + "}";
    private static string DescribeTaskMethods(List<string> methods) =>
        methods.Count == 0 ? "" : ",\"asyncMethods\":" + Encode(methods);
    private static Dictionary<string, TacMember> CreateMembers(string owner, string service, out System.Threading.CancellationTokenSource lifetime)
    {
        lifetime = new();
        var previous = CreatingCancellation;
        CreatingCancellation = lifetime;
        try { return NativeCapabilities.Create(owner, () => TacServices.Members(service)); }
        catch { lifetime.Cancel(); lifetime.Dispose(); throw; }
        finally { CreatingCancellation = previous; CreatingProps = null; }
    }

    private static string Handle(string request)
    {
        try
        {
            var size = Encoding.UTF8.GetByteCount(request);
            if (size > StartUpRequestLimit) return TooLarge("request", size, StartUpRequestLimit);
            var parsed = JsonNode.Parse(request)?.AsObject();
            var operation = parsed?["op"]?.GetValue<string>();
            if (operation != "init" && size > MemberRequestLimit)
                return TooLarge("request to " + (parsed?["name"]?.GetValue<string>() ?? operation), size, MemberRequestLimit);
            if (operation == "host-dispose") { DisposeAll(); return "{\"value\":null}"; }
            var route = parsed?["route"]?.GetValue<string>() ?? "";
            var owner = parsed?["owner"]?.GetValue<string>() ?? "";
            if (!Guid.TryParseExact(owner, "D", out _)) throw new ArgumentException("Missing native owner identity.");
            var service = parsed?["service"]?.GetValue<string>() ?? "";
            if (service.Length == 0) return "{\"error\":\"Missing companion name.\",\"code\":\"TY_NATIVE_SERVICE\"}";
            Activations.TryGetValue(owner, out var existing);
            if (operation == "init" && (existing == null || existing.Route != route || existing.Service != service)) {
                if (existing != null) DisposeOwner(owner);
                CreatingProps = parsed["props"] as JsonObject;
                var table = CreateMembers(owner, service, out var lifetime);
                if (table == null) { lifetime.Cancel(); lifetime.Dispose(); NativeCapabilities.Dispose(owner); return "{\"error\":\"Unknown native service.\",\"code\":\"TY_NATIVE_SERVICE\"}"; }
                if (Activations.Count >= 256) { lifetime.Cancel(); lifetime.Dispose(); NativeCapabilities.Dispose(owner); throw new ArgumentException("Too many native activations."); }
                var activation = new Activation(route, service, owner, table, lifetime);
                Activations[owner] = activation; Executing = activation;
                try {
                    if (parsed["props"] is JsonObject props)
                    {
                        var required = TacServices.Required(service);
                        foreach (var (property, given) in props)
                            if ((required.Contains("*") || required.Contains(property)) && activation.Members.TryGetValue(property, out var filled) && filled.Write != null)
                                filled.Write(Value(given));
                    }
                    foreach (var member in activation.Members.OrderBy(pair => pair.Key, StringComparer.Ordinal).Select(pair => pair.Value))
                        if (member.Lifecycle != null && !member.Destroy) member.Lifecycle();
                    Drain(activation);
                } catch (Exception) { DisposeOwner(owner); throw; }
            }
            if (!Activations.TryGetValue(owner, out var active) || active.Route != route || active.Service != service) throw new ArgumentException("Stale native owner.");
            if (operation == "dispose") { DisposeOwner(owner); return "{\"value\":null}"; }
            Executing = active;
            var members = active.Members;
            if (operation == "init")
            {
                var fields = new List<string>();
                var methods = new List<string>();
                var listeners = new List<string>();
                var asynchronous = new List<string>();
                foreach (var member in members)
                {
                    if (member.Value.Read != null) fields.Add(member.Key);
                    else if (member.Value.Invoke != null) methods.Add(member.Key);
                    else if (member.Value.InvokeAsync != null) { methods.Add(member.Key); asynchronous.Add(member.Key); }
                    else if (member.Value.Listen != null) listeners.Add(DescribeListener(member.Key, member.Value));
                }
                return "{\"value\":{\"fields\":" + Encode(fields) + ",\"methods\":"
                    + Encode(methods) + DescribeTaskMethods(asynchronous) + ",\"listeners\":[" + string.Join(",", listeners) + "]}}";
            }
            var name = parsed?["name"]?.GetValue<string>();
            if (name == null || !members.TryGetValue(name, out var found))
                return "{\"error\":\"Unknown companion member.\"}";
            switch (operation)
            {
                case "event" when found.Listen != null:
                    return ReceiveEvent(found, parsed["value"]);
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
        catch (Exception)
        {
            return "{\"error\":\"Native companion operation failed.\"}";
        }
    }
}

// __TACHYON_NATIVE_ABI__
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

    [UnmanagedCallersOnly(EntryPoint = "tac_native_set_emit")]
    public static unsafe void NativeSetEmit(IntPtr emit) => Tac.SetEmit(emit);
}
