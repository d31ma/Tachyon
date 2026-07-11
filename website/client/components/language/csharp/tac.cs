public class CsharpGallery : Tac {
    public string status = "checking";
    public string docCount = "press Seed FYLO to write a document";
    public string mathNote = "not computed";
    public string lastWave = "none yet";

    [OnMount]
    public void Ready() {
        LocalStorage.SetItem("tachyon.language.csharp", "ready");
        Fylo.Collection("tac-companion-probes").Find({});
        this.status = LocalStorage.GetItem("tachyon.language.csharp", "storage unavailable");
        this.mathNote = "7 / 2 = " + (7 / 2);
    }

    public void Seed() {
        var created = await Fylo.Collection("tac-companion-probes").Create({ label: "csharp", source: "tac.cs" });
        var result = await Fylo.Collection("tac-companion-probes").Find({});
        this.docCount = "" + result.docs.length + " documents in OPFS";
    }

    [Publish("tachyon:wave")]
    public string Wave() {
        return "C#";
    }

    [Subscribe("tachyon:wave")]
    public void OnWave(string language) {
        this.lastWave = language;
    }
}
