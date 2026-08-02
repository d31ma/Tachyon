public static class Companion
{
    public static int Count = 6;
    public static string Label = "from C#";

    public static int Doubled() => Count * 2;

    public static readonly Dictionary<string, TacMember> Tac = new()
    {
        ["count"] = TacMember.Field(() => Count, value => Count = Convert.ToInt32(value)),
        ["label"] = TacMember.Field(() => Label),
        ["doubled"] = TacMember.Method(arguments => Doubled()),
    };
}
