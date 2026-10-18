internal static class TacSocketTransport
{
    internal const int MessageLimit = 65536;
    internal const string Missing = "{\"error\":{\"code\":\"HTTP_MISSING\"}}";
    internal const string TooLarge = "{\"error\":{\"code\":\"HTTP_BODY\"}}";
    internal static Func<string, string, System.Threading.Tasks.Task<string>> Send;
}
