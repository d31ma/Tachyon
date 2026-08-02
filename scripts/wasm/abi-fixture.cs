// A C# browser companion, written as plain C#. There is no tac_invoke here and
// no subset of the language: this is compiled by the .NET wasm publish, and the
// prelude the build appends carries the ABI of ADR 0011.
//
// The class is named Companion because the prelude has to find it, and the
// members are declared because the host must know a field from a method.

public static class Companion
{
    public static int Count = 6;
    public static string Label = "from c#";

    public static int Doubled() => Count * 2;

    public static readonly Dictionary<string, TacMember> Tac = new()
    {
        ["count"] = TacMember.Field(() => Count, value => Count = Convert.ToInt32(value)),
        ["label"] = TacMember.Field(() => Label),
        ["doubled"] = TacMember.Method(arguments => Doubled()),
    };
}
