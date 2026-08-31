#!/usr/bin/env node
// Blackbox gate: compile with the selected ty binary, then execute the staged
// target-native companion ABI. No framework-private source is imported here.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const binary = process.env.TAC_BIN ?? path.join(repo, 'target/release', process.platform === 'win32' ? 'ty.exe' : 'ty');
const target = { darwin: 'macos', linux: 'linux', win32: 'windows' }[process.platform];
assert.ok(target, 'native companion gate requires a desktop host');
const project = mkdtempSync(path.join(tmpdir(), 'ty-native-companion-'));
const write = (name, source) => {
  const file = path.join(project, name);
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, source);
};
const run = (command, args) => {
  const result = spawnSync(command, args, { encoding: 'utf8', timeout: 240000, maxBuffer: 2 * 1024 * 1024 });
  assert.equal(result.status, 0, `${command} failed: ${result.error || result.stderr || result.stdout}`);
  return result.stdout;
};
try {
  write('tac.config.js', "export const application = { id: 'dev.tachyon.companion-gate', name: 'CompanionGate', version: '0.0.1', entryRoute: '/' };\n");
  for (const route of ['', 'second/']) {
    write(`client/pages/${route}tac.html`, '<main><p>{count}</p></main>');
    write(`client/pages/${route}tac.rs`, `#[derive(Default)]
struct Companion {
    count: i64,
}
impl Companion {
    fn doubled(&self) -> i64 { self.count * 2 }
    fn process_id(&self) -> i64 { std::process::id() as i64 }
    fn announce(&self) { tac_publish("native.event", TacValue::Text("received".to_owned())); }
}
`);
  }
  run(binary, ['build', project, '--target', target]);
  const staged = path.join(project, 'dist', target, 'project', 'companion.rs');
  assert.ok(existsSync(staged), 'ty must emit a native companion compilation unit');
  const suffix = process.platform === 'win32' ? '.dll' : process.platform === 'darwin' ? '.dylib' : '.so';
  const library = path.join(project, `probe${suffix}`);
  run('rustc', ['--edition', '2024', '--crate-type', 'cdylib', '-O', staged, '-o', library]);
  const probe = String.raw`
import ctypes, json, os, sys
library = ctypes.CDLL(sys.argv[1])
library.tac_native_invoke.argtypes = [ctypes.c_char_p]
library.tac_native_invoke.restype = ctypes.c_void_p
library.tac_native_free.argtypes = [ctypes.c_void_p]
def invoke_raw(raw):
    answer = library.tac_native_invoke(raw)
    assert answer, 'native ABI returned a null response'
    try:
        return json.loads(ctypes.string_at(answer).decode())
    finally:
        library.tac_native_free(answer)
def invoke(route, op, **values):
    return invoke_raw(json.dumps(dict(route=route, op=op, **values)).encode())
for malformed in [b'{', b'[]', b'x' * 65537]:
    assert 'error' in invoke_raw(malformed), 'malformed/oversized native request accepted'
members = invoke('/', 'init')['value']
assert 'count' in members['fields'] and 'doubled' in members['methods'], members
assert invoke('/', 'get', name='count')['value'] == 0
invoke('/', 'set', name='count', value=7)
assert invoke('/', 'get', name='count')['value'] == 7
assert invoke('/', 'call', name='doubled', args=[])['value'] == 14
assert invoke('/second', 'get', name='count')['value'] == 0, 'route state leaked'
invoke('/second', 'set', name='count', value=19)
assert invoke('/second', 'call', name='doubled', args=[])['value'] == 38
assert invoke('/', 'get', name='count')['value'] == 7, 'route state overwritten'
assert invoke('/', 'call', name='processId', args=[])['value'] == os.getpid(), 'not target-native execution'
assert 'error' in invoke('/', 'call', name='missing', args=[])
assert 'error' in invoke('/', 'unsupported-operation')
assert 'error' in invoke('/missing', 'call', name='doubled', args=[])
assert invoke('/missing', 'init')['value'] == {'fields': [], 'methods': []}
received = []
callback_type = ctypes.CFUNCTYPE(None, ctypes.c_char_p)
@callback_type
def receive(raw):
    received.append(json.loads(raw.decode()))
library.tac_native_set_emit.argtypes = [callback_type]
library.tac_native_set_emit(receive)
invoke('/', 'call', name='announce', args=[])
assert received == [{'name': 'native.event', 'value': 'received'}], received
print('Native companion ABI passed: field read/write, method call, two-route isolation, OS access, publish callback, unknown-route rejection.')
`;
  console.log(run(process.platform === 'win32' ? 'python' : 'python3', ['-c', probe, library]).trim());
} finally {
  rmSync(project, { recursive: true, force: true });
}
