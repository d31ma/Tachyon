#!/usr/bin/env python3
"""Write a real two-route WebView/native companion acceptance project."""

import argparse
from pathlib import Path


def write(root, relative, source):
    destination = root / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(source, encoding="utf-8")


def create(root, platform):
    if any(root.iterdir()):
        raise RuntimeError("the acceptance fixture directory must be empty")
    write(root, "tac.config.js", """export const application = {
  name: 'Native Gate', id: 'dev.tachyon.desktop-gate',
  version: '0.0.1', entryRoute: '/',
};
""")
    write(root, "client/shared/native-gate.css", """body {
  --native-gate: ready; font-family: sans-serif; padding: 24px;
}
button, a { display: block; padding: 8px; margin: 8px; }
""")
    write(root, "client/shared/native-gate.js", "export const marker = 'Shared module ready';\n")
    for route, label, number in [("", "Root", 7), ("items/_id/", "Second", 9)]:
        language = "CSharp" if route and platform == "windows" else "Rust"
        link = '<a href="/">Return root</a>' if route else '<a href="/items/7/">Open second</a>'
        write(root, f"client/pages/{route}tac.html", f"""<!doctype html>
<html><head><title>Native Gate</title>
<link rel="stylesheet" href="/shared/native-gate.css"></head><body>
<main aria-label="{label} route"><h1>{label} route</h1>
<p>{{marker}}</p><p>{label} count {{count}}</p><p>{{status}}</p>
<p>{{platformStatus}}</p><p>{{styleStatus}}</p><p>{{boundaryStatus}}</p>
<p>Publish {{signal}}</p>
<button on:click="verify()">Verify native {label}</button>
{link}<a href="https://example.invalid/">Leave app</a>
</main></body></html>
""")
        write(root, f"client/pages/{route}tac.js", f"""import {{ marker }} from '/shared/native-gate.js';
export default class {{
  marker = marker;
  count = -100;
  status = 'Waiting for native verification';
  platformStatus = '';
  styleStatus = '';
  boundaryStatus = '';
  @subscribe('native.event')
  signal = 'waiting';
  async verify() {{
    this.count = {number};
    this.status = 'Native {language} ' + await this.doubled();
    this.platformStatus = await this.processId() > 0 ? 'OS ready' : 'OS failed';
    const stylesheet = await fetch('/shared/native-gate.css', {{ signal: AbortSignal.timeout(5000) }});
    const stylesheetType = stylesheet.headers.get('content-type')?.split(';')[0].trim();
    const style = getComputedStyle(document.body).getPropertyValue('--native-gate').trim();
    this.styleStatus = stylesheet.ok && stylesheetType === 'text/css' && style === 'ready'
      ? 'Styles ready' : 'Styles failed';
    let routeRejected = false;
    try {{
      const rejected = JSON.parse(await globalThis.__tachyonNativeHostCall(
        'companion.invoke', JSON.stringify({{ route: '/not-this-route', op: 'init' }})));
      routeRejected = Boolean(rejected.error);
    }} catch {{ routeRejected = true; }}
    this.boundaryStatus = routeRejected ? 'Route boundary ready' : 'Route boundary FAILED';
    await this.announce();
  }}
}}
""")
        if language == "Rust":
            write(root, f"client/pages/{route}tac.rs", """#[derive(Default)]
struct Companion {
    count: i64,
}
impl Companion {
    fn doubled(&self) -> i64 { self.count * 2 }
    fn process_id(&self) -> i64 { std::process::id() as i64 }
    fn announce(&self) {
        tac_publish("native.event", TacValue::Text("received".to_owned()));
    }
}
""")
        else:
            write(root, f"client/pages/{route}tac.cs", """public class Companion
{
    public int Count = 0;
    public int Doubled() => Count * 2;
    public int ProcessId() => Environment.ProcessId;
    public void Announce() { Tac.Publish("native.event", "received"); }
}
""")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("platform", choices=["linux", "windows"])
    arguments = parser.parse_args()
    arguments.directory.mkdir(parents=True, exist_ok=True)
    create(arguments.directory, arguments.platform)
