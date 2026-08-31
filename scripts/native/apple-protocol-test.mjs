#!/usr/bin/env node
// Execute generated-source bridge/companion protocol bodies without a GUI.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

assert.equal(process.platform, 'darwin', 'Apple protocol qualification requires Xcode');
const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const native = path.join(repo, 'crates/tachyon-core/src/native');
const temporary = mkdtempSync(path.join(tmpdir(), 'tachyon-apple-protocol-'));
/** @param {string} program @param {string[]} args @param {number} timeout */
const run = (program, args, timeout) => spawnSync(program, args, {
  encoding: 'utf8', timeout, killSignal: 'SIGKILL', maxBuffer: 1024 * 1024,
});
try {
  const helper = path.join(native, 'apple_json.swift');
  const preludes = readFileSync(helper, 'utf8') + '\n' +
    readFileSync(path.join(native, 'prelude.swift'), 'utf8');
  /** @param {'macos'|'ios'} platform */
  const guard = platform => {
    const source = readFileSync(path.join(native, platform + '.rs'), 'utf8');
    const begin = source.indexOf('    func userContentController(');
    assert.ok(begin >= 0, platform + ' host must retain its guarded bridge entry point');
    const end = source.indexOf('\n    }', begin);
    return `final class ${platform}Bridge {\n${source.slice(begin, end + 6)}\n` +
      'func handle(_ capability: String, _ payload: String) -> String { tacNativeInvoke(payload) }\n}\n';
  };
  const fixture = String.raw`
import Foundation
import CoreFoundation

private var count = 1
private var value: Any?
private var calls = 0
func tacRouteMembers(_ route: String) -> [String: TacMember]? {
    guard route == "/" || route == "/other" else { return nil }
    return [
        "count": .field({ route == "/" ? count : 99 }, { count = $0 as? Int ?? -1 }),
        "value": .field({ value }, { value = $0 }),
        "echo": .method({ arguments in calls += 1; return arguments }),
    ]
}
struct WKUserContentController {}
struct TestRequest { let url: URL? }
struct TestFrame { let isMainFrame: Bool; let request: TestRequest }
struct WKScriptMessage { let frameInfo: TestFrame; let body: Any }
func tachyonRoute(_ path: String) -> String? {
    path == "/" || path == "/index.html" ? "/" : path == "/other" ? "/other" : nil
}
func require(_ okay: @autoclosure () -> Bool, _ message: String) {
    if !okay() { FileHandle.standardError.write(Data((message + "\n").utf8)); exit(1) }
}
func response(_ raw: String) -> [String: Any] {
    let answer = tacNativeInvoke(raw)
    guard let parsed = (try? JSONSerialization.jsonObject(with: Data(answer.utf8))) as? [String: Any] else {
        require(false, "Companion produced invalid JSON: " + String(answer.prefix(128))); return [:]
    }
    return parsed
}
func invoke(_ request: [String: Any]) -> [String: Any] {
    let raw = String(data: try! JSONSerialization.data(withJSONObject: request), encoding: .utf8)!
    return response(raw)
}
func bridgeProbe(_ receive: (WKUserContentController, WKScriptMessage, @escaping (Any?, String?) -> Void) -> Void) {
    func check(_ raw: String, _ url: String = "tachyon-app://bundle/index.html", _ main: Bool = true, denied: Bool) {
        var settled = false
        let message = WKScriptMessage(frameInfo: TestFrame(isMainFrame: main, request: TestRequest(url: URL(string: url))),
            body: ["capability": "companion.invoke", "payload": raw])
        receive(WKUserContentController(), message) { answer, error in
            settled = true
            if denied { require(error != nil, "Actual Apple host guard accepted an untrusted or ambiguous request") }
            else {
                require(error == nil, "Actual Apple host guard rejected valid request")
                require((answer as? String) == "{\"value\":1}", "Actual Apple host guard dispatched the wrong route")
            }
        }
        require(settled, "Actual Apple host guard did not settle its reply")
    }
    let valid = #"{"route":"/","op":"get","name":"count"}"#
    check(valid, denied: false)
    check(valid, "https://foreign.example/", denied: true)
    check(valid, "tachyon-app://bundle/index.html", false, denied: true)
    check(valid, "tachyon-app://bundle:444/index.html", denied: true)
    check(valid, "tachyon-app://user@bundle/index.html", denied: true)
    check(#"{"route":"/other","op":"get","name":"count"}"#, denied: true)
    check(#"{"route":"/","route":"/other","op":"get","name":"count"}"#, denied: true)
    check(#"{"route":"/","\u0072oute":"/other","op":"get","name":"count"}"#, denied: true)
}
`;
  const probes = String.raw`
switch CommandLine.arguments.last! {
case "duplicates":
    for raw in [
        #"{"route":"/","route":"/other","op":"get","name":"count"}"#,
        #"{"route":"/","\u0072oute":"/other","op":"get","name":"count"}"#,
        #"{"route":"/","op":"get","op":"call","name":"count"}"#,
    ] { require(response(raw)["error"] != nil, "Ambiguous duplicate protocol keys reached a companion") }
case "unicode":
    require(response(#"{"́":1,"route":"/","op":"get","name":"count"}"#)["value"] as? Int == 1,
        "Valid combining-character JSON did not complete")
    for text in ["́", "😀", "é", "日本語", "\u{0000}", "\\\"}\"route\":\"/other\""] {
        _ = invoke(["route":"/", "op":"set", "name":"value", "value":text])
        require(invoke(["route":"/", "op":"get", "name":"value"])["value"] as? String == text,
            "Unicode or NUL value changed at the Swift boundary")
    }
    _ = response(#"{"route":"/","op":"set","name":"value","value":"\ud83d\ude00"}"#)
    require(value as? String == "😀", "Escaped surrogate pair was not decoded")
case "values":
    _ = invoke(["route":"/", "op":"init"])
    require(calls == 0, "Initializing native members called an application method")
    _ = invoke(["route":"/", "op":"set", "name":"count", "value":7])
    require(count == 7, "Integer field assignment changed its type")
    for number in [0, 1, 7, -2] {
        let result = invoke(["route":"/", "op":"call", "name":"echo", "args":[number]])["value"] as! [Any]
        require(CFGetTypeID(result[0] as! NSNumber) != CFBooleanGetTypeID(), "Integer became a boolean")
    }
    let result = invoke(["route":"/", "op":"call", "name":"echo", "args":[true, NSNull(), [NSNull(), 1], ["nil":NSNull()]]])["value"] as! [Any]
    require(result[0] as? Bool == true && result[1] is NSNull, "Boolean/null value changed")
    require((result[2] as? [Any])?.first is NSNull, "Nested array null changed")
    require((result[3] as? [String:Any])?["nil"] is NSNull, "Nested dictionary null disappeared")
    let fractional = invoke(["route":"/", "op":"call", "name":"echo", "args":[0.25, -1.5]])["value"] as! [Double]
    require(fractional == [0.25, -1.5], "Fractional values changed")
    let boundary = "{\"route\":\"/\",\"op\":\"init\",\"x\":" + String(repeating:"[",count:63) + "0" + String(repeating:"]",count:63) + "}"
    require(response(boundary)["error"] == nil, "Valid depth-64 request was refused")
    let prefix = "{\"route\":\"/\",\"op\":\"init\",\"padding\":\""
    let bytesBoundary = prefix + String(repeating:"a",count:65536-prefix.utf8.count-2) + "\"}"
    require(response(bytesBoundary)["error"] == nil, "Valid 64-KiB request was refused")
    for raw in ["{", "[]", "{\"route\":\"/\",}", String(repeating:"x", count:65537),
        "{\"route\":\"/\",\"op\":\"init\",\"x\":" + String(repeating:"[",count:65) + "0" + String(repeating:"]",count:65) + "}"] {
        require(response(raw)["error"] != nil, "Malformed/oversized/deep JSON was accepted")
    }
case "bridge":
    bridgeProbe(macosBridge().userContentController)
    bridgeProbe(iosBridge().userContentController)
default: require(false, "Unknown Apple protocol probe")
}
print("PASS: " + CommandLine.arguments.last!)
`;
  const source = path.join(temporary, 'main.swift');
  const binary = path.join(temporary, 'probe');
  writeFileSync(source, preludes + fixture + guard('macos') + guard('ios') + probes);
  const compiled = run('/usr/bin/xcrun', ['swiftc', source, '-o', binary], 60000);
  assert.equal(compiled.status, 0, String(compiled.error || compiled.stderr));
  let failures = 0;
  for (const probe of ['duplicates', 'unicode', 'values', 'bridge']) {
    const result = run(binary, [probe], 5000);
    if (result.status !== 0) {
      failures++;
      console.error(`FAIL: ${probe}: ${result.error || result.stderr || result.signal}`);
    } else console.log(result.stdout.trim());
  }
  assert.equal(failures, 0, `${failures} Apple protocol security regressions failed`);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
