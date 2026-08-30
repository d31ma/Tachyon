// Compiled beside a `yon.cs` handler. Everything here is the protocol.
//
// The author writes a class and methods. Reading standard input, dispatching
// on the method and writing the response envelope is Tachyon's half.
//
// A second file rather than an append, because a C# handler already builds
// through a generated project and a project takes as many sources as it likes.
// Nothing here needs to be in the handler's own file.
//
// System.Text.Json is in the box, so no parser is written by hand — unlike
// Java and Kotlin, whose standard libraries have none.

using System;
using System.Buffers.Binary;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Threading.Tasks;

/// <summary>The stereotypes, so a handler needs no import to declare a layer.</summary>
[AttributeUsage(AttributeTargets.Class)]
sealed class ControllerAttribute : Attribute { }

[AttributeUsage(AttributeTargets.Class)]
sealed class ServiceAttribute : Attribute { }

[AttributeUsage(AttributeTargets.Class)]
sealed class RepositoryAttribute : Attribute { }

[AttributeUsage(AttributeTargets.Class)]
sealed class ClientAttribute : Attribute { }

[AttributeUsage(AttributeTargets.Class)]
sealed class DelegateAttribute : Attribute { }

/// <summary>Marks a method as a proxy for a program Yon does not run.
///
/// C# is one of the four languages here whose annotations cannot do the work
/// themselves — an attribute is inert, so the dispatch below checks for it and
/// the annotated body is never called.</summary>
[AttributeUsage(AttributeTargets.Method)]
sealed class RelayAttribute : Attribute
{
    public RelayAttribute(params string[] command) => Command = command;

    public string[] Command { get; }
}

/// <summary>Marks a method that answers more than once.
///
/// Declared because C# will not compile an attribute it cannot resolve, and
/// read by Tachyon before the compiler runs. The dispatch below never asks for
/// it — what it looks at is whether the method returned a sequence — and the
/// two are checked against each other at build time.</summary>
[AttributeUsage(AttributeTargets.Method)]
sealed class StreamAttribute : Attribute { }

/// <summary>JSON, under the name every other language here gives it.
///
/// C# has one in the box, unlike Java, Kotlin and Rust, so this is a thin
/// wrapper rather than a parser. It exists so a handler reads the same in
/// every language — `Json.Write(…)` and `Json.Parse(…)` — and so a C# handler
/// does not have to name a namespace to answer with an object.</summary>
static class Json
{
    /// <summary>Writes one value as JSON. An anonymous object is the readable
    /// way to build one: `Json.Write(new { products = new[] { "anvil" } })`.</summary>
    public static string Write(object value) => JsonSerializer.Serialize(value);

    /// <summary>Reads one JSON document. Anything malformed reads as an empty
    /// object rather than throwing: a handler asking for a field that is not
    /// there wants an empty answer, not a failure mid-request.</summary>
    public static JsonNode Parse(string source)
    {
        if (string.IsNullOrWhiteSpace(source)) return new JsonObject();
        try
        {
            return JsonNode.Parse(source) ?? new JsonObject();
        }
        catch (JsonException)
        {
            return new JsonObject();
        }
    }
}

/// <summary>One request, as the protocol delivers it.</summary>
sealed class YonRequest
{
    private readonly JsonObject raw;

    internal YonRequest(JsonNode parsed)
    {
        raw = parsed as JsonObject ?? new JsonObject();
    }

    /// <summary>The HTTP method, upper case.</summary>
    public string Method() => Text(raw["method"]);

    /// <summary>The route as matched, with its parameters still in it.</summary>
    public string Route() => Text(raw["route"]);

    /// <summary>One bound route parameter, or empty when the route has none.</summary>
    public string Parameter(string name) =>
        raw["parameters"] is JsonObject parameters ? Text(parameters[name]) : "";

    /// <summary>The request body as text, empty when none was sent.</summary>
    public string Body() =>
        raw["body"] is JsonObject body ? Text(body["data"]) : "";

    /// <summary>The request body already parsed, for the common case of JSON
    /// in. Named `Body` in every language; this is the parsed half.</summary>
    public JsonNode Parsed() => Json.Parse(Body());

    /// <summary>The whole request, for anything the accessors do not cover.</summary>
    public JsonObject Raw() => raw;

    private static string Text(JsonNode node) =>
        node is null ? "" : node.GetValue<object>()?.ToString() ?? node.ToJsonString();
}

/// <summary>One response, built the way the other layers build theirs.</summary>
sealed class YonResponse
{
    private int status;
    private readonly Dictionary<string, List<string>> headers = new();
    private readonly string body;

    private YonResponse(int status, string body)
    {
        this.status = status;
        this.body = body;
    }

    /// <summary>A JSON body with a 200. The common case, so it is the short one.</summary>
    public static YonResponse Json(string body)
    {
        var response = new YonResponse(200, body);
        response.headers["content-type"] = new List<string> { "application/json" };
        return response;
    }

    /// <summary>A response with no body, for a 204 or a redirect.</summary>
    public static YonResponse Empty(int status) => new(status, "");

    public YonResponse Status(int value)
    {
        status = value;
        return this;
    }

    /// <summary>Replaces every header at once, for a delegate answering with
    /// its own. Without this the assumed content type would survive alongside
    /// the one the delegate actually set.</summary>
    internal YonResponse Headers(Dictionary<string, List<string>> replacement)
    {
        headers.Clear();
        foreach (var pair in replacement) headers[pair.Key] = pair.Value;
        return this;
    }

    public YonResponse Header(string name, string value)
    {
        // A header value is a list, because one header may repeat.
        if (!headers.TryGetValue(name, out var values))
        {
            values = new List<string>();
            headers[name] = values;
        }
        values.Add(value);
        return this;
    }

    internal string Envelope()
    {
        var envelope = new JsonObject
        {
            ["status"] = status,
            ["body"] = body,
        };
        var written = new JsonObject();
        foreach (var pair in headers)
        {
            var list = new JsonArray();
            foreach (var value in pair.Value) list.Add(value);
            written[pair.Key] = list;
        }
        envelope["headers"] = written;
        return envelope.ToJsonString();
    }
}

/// <summary>One streamed frame on standard output.
///
/// A stream is length-prefixed where a single response is not: the reader has
/// to know where each frame ends, because more follow. `[4-byte big-endian
/// length][UTF-8 JSON]` is the shape the two adapters already write, and the
/// reader that consumes it never asks which language wrote it.</summary>
static class YonStream
{
    /// <summary>The reader refuses a larger frame, so refusing it here says
    /// why.</summary>
    const int MaxFrameBytes = 16 * 1024 * 1024;

    /// <summary>The raw stream, not Console.Out: a writer would apply the
    /// console encoding to bytes that are already exactly what they should
    /// be.</summary>
    static readonly Stream Output = Console.OpenStandardOutput();

    public static bool Event(string requestId, object value)
    {
        var frame = new JsonObject
        {
            ["protocol_version"] = 1,
            ["kind"] = "event",
            ["request_id"] = requestId,
            ["body"] = new JsonObject
            {
                ["encoding"] = "utf8",
                ["data"] = Json.Write(value),
            },
        };
        return Write(requestId, frame);
    }

    /// <summary>An error frame ends the stream. The reader turns it into a
    /// failure rather than presenting it as data, which is the difference
    /// between a handler that stopped and one that had nothing more to
    /// say.</summary>
    public static void Failure(string requestId, string message)
    {
        Write(requestId, Envelope(requestId, "TY2204", message));
    }

    static JsonObject Envelope(string requestId, string code, string message) =>
        new()
        {
            ["protocol_version"] = 1,
            ["kind"] = "response",
            ["request_id"] = requestId,
            ["status"] = 500,
            ["headers"] = new JsonObject(),
            ["error"] = new JsonObject
            {
                ["code"] = code,
                ["message"] = message,
                ["retryable"] = false,
            },
        };

    static bool Write(string requestId, JsonObject frame)
    {
        var payload = Encoding.UTF8.GetBytes(frame.ToJsonString());
        if (payload.Length > MaxFrameBytes)
        {
            var refusal = Encoding.UTF8.GetBytes(
                Envelope(requestId, "TY2203", "Streamed event exceeds the protocol frame limit.")
                    .ToJsonString());
            Emit(refusal);
            return false;
        }
        Emit(payload);
        return true;
    }

    static void Emit(byte[] payload)
    {
        var prefix = new byte[4];
        BinaryPrimitives.WriteUInt32BigEndian(prefix, (uint)payload.Length);
        Output.Write(prefix, 0, prefix.Length);
        Output.Write(payload, 0, payload.Length);
        // Flushed per frame: a stream whose events arrive together at the end
        // is a slow response wearing a stream's clothes.
        Output.Flush();
    }
}

/// <summary>The entry point Tachyon supplies, so the handler writes none.</summary>
static class Yon
{
    const int MaxRelayStdoutBytes = 16 * 1024 * 1024;
    const int MaxRelayStderrBytes = 64 * 1024;

    sealed class RelayOutput
    {
        public byte[] Bytes { get; init; } = Array.Empty<byte>();
        public bool Overflow { get; init; }
    }

    static async System.Threading.Tasks.Task<RelayOutput> Drain(
        System.IO.Stream pipe,
        int limit)
    {
        using var kept = new System.IO.MemoryStream(Math.Min(limit, 8192));
        var chunk = new byte[8192];
        var overflow = false;
        while (true)
        {
            var count = await pipe.ReadAsync(chunk.AsMemory());
            if (count == 0) break;
            var remaining = Math.Max(0, limit - (int)kept.Length);
            await kept.WriteAsync(chunk.AsMemory(0, Math.Min(count, remaining)));
            overflow |= count > remaining;
        }
        return new RelayOutput { Bytes = kept.ToArray(), Overflow = overflow };
    }

    /// <summary>Runs a handler written in a language Yon does not run.
    ///
    /// Yon runs the eight languages that can declare a layer. Go, Ruby, Elixir
    /// and the rest cannot, so they are not routes — but they are still
    /// programs, and a program that speaks Handler Protocol v1 on standard
    /// input and output is exactly what Yon spawns anyway.
    ///
    /// The command is explicit rather than inferred from the file name: a
    /// compiled language has no interpreter to infer. The working directory is
    /// the project root, so a project-relative path reads as written.</summary>
    public static YonResponse Relay(IReadOnlyList<string> command, YonRequest request)
    {
        if (command.Count == 0) return Failed("A delegate command cannot be empty.");
        var program = command[0];
        try
        {
            // UseShellExecute stays false, so nothing is passed through a
            // shell: a route parameter that reached a command line would be an
            // injection.
            var start = new System.Diagnostics.ProcessStartInfo(program)
            {
                RedirectStandardInput = true,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                UseShellExecute = false,
            };
            for (var index = 1; index < command.Count; index += 1)
            {
                start.ArgumentList.Add(command[index]);
            }
            using var child = System.Diagnostics.Process.Start(start);
            if (child is null) return Failed("Delegate could not be started.");
            var stdoutTask = Drain(child.StandardOutput.BaseStream, MaxRelayStdoutBytes);
            var stderrTask = Drain(child.StandardError.BaseStream, MaxRelayStderrBytes);
            // Closed before the output is read, so the child sees end of input
            // rather than waiting for more while this side waits for an answer.
            child.StandardInput.Write(request.Raw().ToJsonString());
            child.StandardInput.Close();
            var requested = request.Raw()["deadline_ms"]?.GetValue<long>() ?? 30_000;
            var timeout = TimeSpan.FromMilliseconds(Math.Clamp(requested, 1, 300_000));
            try
            {
                child.WaitForExitAsync().WaitAsync(timeout).GetAwaiter().GetResult();
            }
            catch (TimeoutException)
            {
                child.Kill(entireProcessTree: true);
                child.WaitForExit(1000);
                return Failed("Delegate invocation failed.");
            }
            System.Threading.Tasks.Task.WhenAll(stdoutTask, stderrTask)
                .WaitAsync(TimeSpan.FromSeconds(1)).GetAwaiter().GetResult();
            var stdout = stdoutTask.Result;
            var stderr = stderrTask.Result;
            if (child.ExitCode != 0 || stdout.Overflow || stderr.Overflow)
            {
                return Failed("Delegate invocation failed.");
            }
            if (JsonNode.Parse(stdout.Bytes) is not JsonObject envelope)
            {
                return Failed("Delegate returned an invalid response.");
            }
            var response = YonResponse.Json(envelope["body"]?.GetValue<string>() ?? "");
            if (envelope["status"] is JsonValue status)
            {
                response = response.Status(status.GetValue<int>());
            }
            // The headers the delegate set replace the assumed content type,
            // because a delegate that answered with a header meant that header.
            if (envelope["headers"] is JsonObject written && written.Count > 0)
            {
                response = response.Headers(new Dictionary<string, List<string>>());
                foreach (var pair in written)
                {
                    var values = pair.Value is JsonArray list
                        ? list
                        : new JsonArray(pair.Value?.DeepClone());
                    foreach (var value in values)
                    {
                        response = response.Header(pair.Key, value?.GetValue<string>() ?? "");
                    }
                }
            }
            return response;
        }
        catch (Exception)
        {
            return Failed("Delegate invocation failed.");
        }
    }

    /// <summary>A delegate that could not be run answers 502, the same as any
    /// other upstream that did not reply. Delegate stderr and process errors
    /// are diagnostic-only and never become client response data.</summary>
    static YonResponse Failed(string reason) =>
        YonResponse.Json(JsonSerializer.Serialize(new { error = reason })).Status(502);

    /// <summary>Async because a stream may be an IAsyncEnumerable, and
    /// awaiting one needs `await foreach`. A handler that awaits per event is
    /// the realistic case for a stream.</summary>
    static async Task Main()
    {
        var raw = Console.In.ReadToEnd();
        var parsed = string.IsNullOrWhiteSpace(raw) ? new JsonObject() : JsonNode.Parse(raw);
        var request = new YonRequest(parsed);

        object result;
        // Found by reflection rather than by a fixed list, so a method the
        // handler does not declare answers 405 without being written.
        var method = typeof(__YON_CONTROLLER__).GetMethod(
            request.Method(),
            BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Static,
            null,
            new[] { typeof(YonRequest) },
            null);
        if (method is null)
        {
            result = YonResponse.Empty(405);
        }
        else
        {
            try
            {
                result = method.Invoke(null, new object[] { request });
            }
            catch (TargetInvocationException failed)
            {
                var cause = failed.InnerException ?? failed;
                result = YonResponse
                    .Json(JsonSerializer.Serialize(new { error = cause.Message }))
                    .Status(500);
            }
        }

        // A method that yields returns a sequence, and a sequence is a stream:
        // each value becomes one frame and end of stream is end of process,
        // because the reader takes EOF as the close.
        //
        // `[Stream]` on the method is what told the server to read frames at
        // all, and Tachyon refuses the two to disagree — so a sequence here
        // means the other end is already reading them.
        //
        // Both forms, because `yield return` gives an IEnumerable and the
        // asynchronous spelling gives an IAsyncEnumerable, and a handler that
        // awaits per event is the realistic case for a stream.
        var id = request.Raw()["request_id"]?.ToString() ?? "";
        if (result is IAsyncEnumerable<object> asynchronous)
        {
            try
            {
                await foreach (var value in asynchronous)
                {
                    if (!YonStream.Event(id, value)) return;
                }
            }
            catch (Exception failed)
            {
                YonStream.Failure(id, failed.Message);
            }
            return;
        }
        // A string is enumerable and is not a stream, and neither is the
        // response object itself.
        if (result is IEnumerable sequence && result is not string && result is not YonResponse)
        {
            try
            {
                foreach (var value in sequence)
                {
                    if (!YonStream.Event(id, value)) return;
                }
            }
            catch (Exception failed)
            {
                YonStream.Failure(id, failed.Message);
            }
            return;
        }
        Console.Out.Write(((YonResponse)result).Envelope());
    }
}
