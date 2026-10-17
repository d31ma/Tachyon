#!/usr/bin/env python3

import argparse
from pathlib import Path


def write(root, relative, source):
    destination = root / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(source, encoding="utf-8")


def manifest(platform):
    visibility = ""
    if platform == "windows":
        visibility = ',\n    "visibility": { "check": false },\n    "scripts": ["/shared/GatePageErrors.js"]'
    return f"""{{
  "name": "Native Gate",
  "start_url": "/",
  "tac": {{
    "application": {{ "id": "dev.tachyon.desktop-gate", "version": "0.0.1" }},
    "styles": ["/shared/native-gate.css"]{visibility}
  }}
}}
"""


def write_shared_files(root, platform):
    write(root, "client/manifest.json", manifest(platform))
    write(root, "client/models/NativeEvent.js", """/**
 * @typedef {Object} NativeEvent
 * @property {string} text
 */
export {}
""")
    write(root, "client/models/NativeEvent.rs", "pub struct NativeEvent {\n    pub text: String,\n}\n")
    write(root, "client/shared/native-gate.css", """body {
  --native-gate: ready; font-family: sans-serif;
}
""")
    write(root, "client/shared/utils/NativeGateUtils.js", """export default class NativeGateUtils {
  /** @returns {string} */
  static marker() {
    return 'Shared module ready'
  }

  /**
   * @param {string} language
   * @param {number} doubled
   * @param {number} processId
   * @param {string} service
   * @returns {Promise<string[]>}
   */
  static async checks(language, doubled, processId, service) {
    return [
      `Native ${language} ${doubled}`,
      NativeGateUtils._platformStatus(processId),
      await NativeGateUtils._styleStatus(),
      await NativeGateUtils._boundaryStatus(service),
    ]
  }

  /**
   * @param {number} processId
   * @returns {string}
   */
  static _platformStatus(processId) {
    return processId > 0 ? 'OS ready' : 'OS failed'
  }

  /** @returns {Promise<string>} */
  static async _styleStatus() {
    const ready = [await NativeGateUtils._stylesheetServed(), NativeGateUtils._styleApplied()].every(Boolean)
    return ready ? 'Styles ready' : 'Styles failed'
  }

  /** @returns {Promise<boolean>} */
  static async _stylesheetServed() {
    const stylesheet = await fetch('/shared/native-gate.css', { signal: AbortSignal.timeout(5000) })
    const type = String(stylesheet.headers.get('content-type')).split(';')[0].trim()
    return [stylesheet.ok, type === 'text/css'].every(Boolean)
  }

  /** @returns {boolean} */
  static _styleApplied() {
    return getComputedStyle(document.body).getPropertyValue('--native-gate').trim() === 'ready'
  }

  /**
   * @param {string} service
   * @returns {Promise<string>}
   */
  static async _boundaryStatus(service) {
    const rejected = await NativeGateUtils._foreignRouteRejected(service)
    return rejected ? 'Route boundary ready' : 'Route boundary FAILED'
  }

  /**
   * @param {string} service
   * @returns {Promise<boolean>}
   */
  static async _foreignRouteRejected(service) {
    const request = JSON.stringify({ route: '/not-this-route', service, op: 'init' })
    try {
      const answer = JSON.parse(await globalThis.__tachyonNativeHostCall('companion.invoke', request))
      return Boolean(answer.error)
    } catch {
      return true
    }
  }
}
""")
    write(root, "client/shared/utils/NativeServiceUtils.rs", """use crate::models::NativeEvent;

pub struct NativeServiceUtils;

impl NativeServiceUtils {
    const RECEIVED: &str = "received";

    pub fn doubled(count: i64) -> i64 {
        count * 2
    }

    pub fn process_id() -> i64 {
        i64::from(std::process::id())
    }

    pub fn announce(publisher: &ApplicationEventPublisher) -> String {
        let event = NativeEvent { text: Self::RECEIVED.to_owned() };
        publisher.publish_event(event, false);
        Self::RECEIVED.to_owned()
    }
}
""")


def component_view(label, instance, link):
    return f"""<main aria-label="{label} route"><h1>{label} route</h1>
<p>{label} count {{{instance}.count}}</p>
<loop :for="check of {instance}.checks"><p>{{check}}</p></loop>
<p>Publish {{{instance}.signal}}</p>
<button on:click="{instance}.verify()">Verify native {label}</button>
{link}<a href="https://example.invalid/">Leave app</a>
</main>
"""


def component_style():
    return """main { padding: 24px; }
button, a { display: block; padding: 8px; margin: 8px; }
"""


def browser_service(service, instance, number, language):
    return f"""@Service
export default class {service} {{
  count = -100
  checks = [NativeGateUtils.marker()]
  signal = 'waiting'

  /** @param {{NativeEvent}} event */
  @OnEvent
  receive(event) {{
    this.signal = event.text
  }}

  async verify() {{
    this.count = {number}
    this.checks = await NativeGateUtils.checks('{language}', await this.doubled(), await this.processId(), '{instance}')
    await this.announce()
  }}
}}
"""


def rust_service(service):
    source = """#[Service]
#[derive(Default)]
struct __SERVICE__ {
    count: i64,
    application_event_publisher: ApplicationEventPublisher,
}

impl __SERVICE__ {
    fn doubled(&self) -> i64 {
        NativeServiceUtils::doubled(self.count)
    }

    fn process_id(&self) -> i64 {
        NativeServiceUtils::process_id()
    }

    fn announce(&self) -> String {
        NativeServiceUtils::announce(&self.application_event_publisher)
    }
}
"""
    return source.replace("__SERVICE__", service)


def csharp_service(service):
    source = """using Models;

[Service]
public class __SERVICE__
{
    private const string Received = "received";
    public int Count = 0;
    private readonly ApplicationEventPublisher applicationEventPublisher;

    public int Doubled() => Count * 2;

    public int ProcessId() => Environment.ProcessId;

    public string Announce()
    {
        applicationEventPublisher.PublishEvent(new NativeEvent(Received));
        return Received;
    }
}
"""
    return source.replace("__SERVICE__", service)


def write_page_errors(root):
    write(root, "client/shared/GatePageErrors.js", """export default class GatePageErrors {
  static start() {
    addEventListener('error', (event) => GatePageErrors.report(`${event.message} ${event.filename}:${event.lineno}`))
    addEventListener('unhandledrejection', (event) => GatePageErrors.report(String(event.reason?.stack || event.reason)))
  }

  /** @param {string} text */
  static report(text) {
    document.title = `Page error: ${text}`.slice(0, 500)
  }
}

GatePageErrors.start()
""")


def write_route(root, platform, route, label, number):
    language = "Rust"
    if route and platform == "windows":
        language = "CSharp"
    component = f"gate/{label.lower()}"
    service = f"Gate{label}Service"
    instance = f"gate{label}"
    link = '<a href="/items/7/">Open second</a>'
    if route:
        link = '<a href="/">Return root</a>'
    write(root, f"client/pages/{route}tac.html", f'<gate-{label.lower()} />\n')
    write(root, f"client/components/{component}/tac.html", component_view(label, instance, link))
    write(root, f"client/components/{component}/tac.css", component_style())
    write(root, f"client/services/{service}.js", browser_service(service, instance, number, language))
    if language == "Rust":
        write(root, f"client/services/{service}.rs", rust_service(service))
    else:
        write(root, f"client/services/{service}.cs", csharp_service(service))
        write(root, "client/models/NativeEvent.cs", "public record NativeEvent(string Text);\n")


def create(root, platform):
    if any(root.iterdir()):
        raise RuntimeError("the acceptance fixture directory must be empty")
    write_shared_files(root, platform)
    if platform == "windows":
        write_page_errors(root)
    write_route(root, platform, "", "Root", 7)
    write_route(root, platform, "items/_id/", "Second", 9)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("platform", choices=["linux", "windows"])
    arguments = parser.parse_args()
    arguments.directory.mkdir(parents=True, exist_ok=True)
    create(arguments.directory, arguments.platform)


if __name__ == "__main__":
    main()
