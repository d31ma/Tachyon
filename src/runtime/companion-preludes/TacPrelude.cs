// Tachyon editor prelude for C# Tac companions.
//
// Never compiled or bundled by Tachyon (only `tac.cs` beside a `tac.html` is
// a companion). Copy this file next to your companion so IDE tooling
// resolves the implicit prelude symbols.
//
// ponytail: a real C# compiler still rejects Tac dialect forms such as bare
// FetchAsync(...) and `{}` query arguments; this stub only fixes symbol
// resolution. C# companions are a Tac dialect, not real C# — see the
// portable companion subset reference in the Tachyon README.

using System;

public class Tac {
    public bool Publish(string name, object value = null) => false;
    public object Env(string key, object fallback = null) => fallback;
    public void Rerender() {}
}

[AttributeUsage(AttributeTargets.All)] public class PublishAttribute : Attribute { public PublishAttribute(string name = "") {} }
[AttributeUsage(AttributeTargets.All)] public class SubscribeAttribute : Attribute { public SubscribeAttribute(string name = "") {} }
[AttributeUsage(AttributeTargets.All)] public class OnMountAttribute : Attribute {}

public static class LocalStorage {
    public static string GetItem(string key, string fallback = "") => fallback;
    public static void SetItem(string key, object value) {}
    public static void RemoveItem(string key) {}
}

public static class SessionStorage {
    public static string GetItem(string key, string fallback = "") => fallback;
    public static void SetItem(string key, object value) {}
    public static void RemoveItem(string key) {}
}

public static class Navigator {
    public static string Language() => "";
    public static bool IsOnline() => false;
}

public static class Location {
    public static string Href() => "";
    public static string Origin() => "";
}

public class FyloCollection {
    public object Find(object query) => null;
    public object Get(object id) => null;
    public object Create(object document = null) => null;
    public object Put(object document) => null;
    public object Delete(object id) => null;
}

public static class Fylo {
    public static FyloCollection Collection(string name) => new FyloCollection();
}

public class AppInfo { public string name = ""; public string version = ""; }

public static class App {
    public static bool IsAvailable() => false;
    public static AppInfo InfoAsync() => new AppInfo();
}

public static class Clipboard {
    public static string GetTextAsync() => "";
    public static void SetTextAsync(string text) {}
}

public static class FileSystem {
    public static string ReadTextAsync(string path) => "";
    public static void WriteTextAsync(string path, string text) {}
    public static object ReadDirAsync(string path) => null;
    public static object PathsAsync() => null;
}

public static class Shell {
    public static object ExecAsync(string command, string[] args = null, string cwd = null) => null;
}

public static class Browser {
    public static void OpenAsync(string url) {}
}

public static class Share {
    public static void TextAsync(string text, string title = null) {}
}

public static class Haptics {
    public static void ImpactAsync() {}
}

public static class FilePicker {
    public static string OpenTextAsync() => "";
    public static void SaveTextAsync(string name, string text) {}
}

public static class Capabilities {
    public static bool Supports(string capability) => false;
    public static string StateAsync(string capability) => "unsupported";
}

public static class Secrets { public static string GetAsync(string key) => null; public static void SetAsync(string key, string value) {} public static void DeleteAsync(string key) {} }
public static class Auth { public static object VerifyUserAsync(string reason) => null; }
public static class Geolocation { public static object CurrentAsync(object options = null) => null; }
public static class Notifications { public static void ShowAsync(string title, object options = null) {} }
public static class Media { public static object GetUserMediaAsync(object constraints) => null; }
