using System.Collections.Generic;
using System.Diagnostics;
using System.Text.Json;

public class CSharpFyloRepository
{
    private readonly string _root = Environment.GetEnvironmentVariable("FYLO_ROOT") ?? Path.Combine(Environment.CurrentDirectory, "db");
    private readonly string _schemaDir = Environment.GetEnvironmentVariable("FYLO_SCHEMA_DIR") ?? Path.Combine(Environment.CurrentDirectory, "db", "schemas");
    private readonly string? _executable = Environment.GetEnvironmentVariable("FYLO_EXEC_PATH");

    public Dictionary<string, object?> ReadCurrentSchema()
    {
        Machine(new Dictionary<string, object?> { ["op"] = "schemaCurrent", ["collection"] = "fylo-demo-items", ["schemaDir"] = _schemaDir });
        Machine(new Dictionary<string, object?> { ["op"] = "schemaHistory", ["collection"] = "fylo-demo-items", ["schemaDir"] = _schemaDir });
        return Summary("fylo-demo-items", new[] { "schemaCurrent", "schemaHistory" }, 2);
    }

    private JsonElement Machine(Dictionary<string, object?> request)
    {
        var command = _executable is { Length: > 0 } ? _executable : "bunx";
        var arguments = _executable is { Length: > 0 }
            ? $"exec --request - --root {Quote(_root)}"
            : $"--bun fylo.exec exec --request - --root {Quote(_root)}";
        var process = new Process
        {
            StartInfo = new ProcessStartInfo(command, arguments)
            {
                RedirectStandardInput = true,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                UseShellExecute = false,
            },
        };
        process.Start();
        process.StandardInput.Write(JsonSerializer.Serialize(request));
        process.StandardInput.Close();
        var stdout = process.StandardOutput.ReadToEnd();
        var stderr = process.StandardError.ReadToEnd();
        process.WaitForExit();
        if (process.ExitCode != 0) throw new Exception(string.IsNullOrWhiteSpace(stderr) ? stdout : stderr);
        using var document = JsonDocument.Parse(string.IsNullOrWhiteSpace(stdout) ? "{}" : stdout);
        if (!document.RootElement.TryGetProperty("ok", out var ok) || !ok.GetBoolean())
            throw new Exception(document.RootElement.TryGetProperty("error", out var error) ? error.ToString() : "fylo.exec returned an error");
        return document.RootElement.GetProperty("result").Clone();
    }

    private static Dictionary<string, object?> Summary(string collection, string[] operations, int count)
    {
        return new Dictionary<string, object?>
        {
            ["collection"] = collection,
            ["operations"] = operations,
            ["resultCount"] = count.ToString(),
        };
    }

    private static string Quote(string value) => $"\"{value.Replace("\"", "\\\"")}\"";
}
