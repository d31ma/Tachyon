// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { chmod, mkdtemp, rm } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import HandlerAdapter from '../../src/server/process/handler-adapter.js';
import Pool from '../../src/server/process/process-pool.js';
import Yon from '../../src/server/yon.js';
import YonCompiledRunner from '../../src/server/process/adapters/yon-compiled-runner.js';

/** @type {string[]} */
const tempDirs = [];

afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});

/** @param {ReadableStream<Uint8Array>} stream */
async function text(stream) {
    return new Response(stream).text();
}

/** @param {string} output */
function responseBody(output) {
    const start = output.indexOf(Yon.RESPONSE_START);
    if (start === -1)
        return output;
    const bodyStart = start + Yon.RESPONSE_START.length;
    const end = output.indexOf(Yon.RESPONSE_END, bodyStart);
    return end === -1 ? output : output.slice(bodyStart, end);
}

/**
 * @param {string} source
 * @returns {Promise<string>}
 */
async function writeExecutableHandler(source) {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-handler-adapter-'));
    tempDirs.push(root);
    const handlerPath = path.join(root, 'GET');
    await Bun.write(handlerPath, source);
    await chmod(handlerPath, 0o755);
    return handlerPath;
}

/**
 * @param {string} command
 * @returns {boolean}
 */
function commandAvailable(command) {
    try {
        const probe = Bun.spawnSync({
            cmd: [command, '--version'],
            stdout: 'pipe',
            stderr: 'pipe',
        });
        return probe.exitCode === 0;
    }
    catch {
        return false;
    }
}

/**
 * @param {() => Promise<void>} callback
 * @returns {Promise<void>}
 */
async function withProductionEnvironment(callback) {
    const previous = process.env.NODE_ENV;
    process.env.NODE_ENV = 'production';
    try {
        await callback();
    }
    finally {
        if (previous === undefined)
            delete process.env.NODE_ENV;
        else
            process.env.NODE_ENV = previous;
    }
}

/**
 * @param {(cmd: string[], cwd: string, input: string | null) => Promise<string>} spy
 * @param {() => Promise<void>} callback
 * @returns {Promise<void>}
 */
async function withRunCommandSpy(spy, callback) {
    const original = YonCompiledRunner.runCommand;
    YonCompiledRunner.runCommand = spy;
    try {
        await callback();
    }
    finally {
        YonCompiledRunner.runCommand = original;
    }
}

test('knownLanguages lists the ergonomic adapters, not a supported set', () => {
    // These are conveniences (write class Handler, skip the stdin/stdout
    // glue), not a closed list — any executable handler in any language is
    // a valid route through the universal path.
    for (const language of ['javascript', 'typescript', 'python', 'ruby', 'php', 'dart', 'java', 'csharp', 'cpp', 'rust'])
        expect(HandlerAdapter.knownLanguages).toContain(language);
});

test('runs JavaScript class-per-route handlers without user stdin/stdout code', async () => {
    const handlerPath = await writeExecutableHandler(`#!/usr/bin/env bun
export class Handler {
  static async GET(request) {
    return { message: 'ok', requestId: request.context.requestId }
  }
}
`);
    const cmd = Pool.resolveHandlerCommand(handlerPath);
    expect(cmd[1]).toContain('yon-js-runner.js');
    const proc = Bun.spawn({
        cmd,
        stdin: 'pipe',
        stdout: 'pipe',
        stderr: 'pipe',
    });
    proc.stdin.write(JSON.stringify({ method: 'GET', context: { requestId: 'abc' } }));
    proc.stdin.end();
    const [stdout, stderr, exitCode] = await Promise.all([text(proc.stdout), text(proc.stderr), proc.exited]);
    expect(exitCode).toBe(0);
    expect(stderr).toBe('');
    expect(JSON.parse(responseBody(stdout))).toEqual({ message: 'ok', requestId: 'abc' });
});

test('resolves extensioned route modules without shebangs', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-extension-adapter-'));
    tempDirs.push(root);
    const handlerPath = path.join(root, 'yon.js');
    await Bun.write(handlerPath, `export class Handler {
  static async GET(request) {
    return { message: 'ok', requestId: request.context.requestId }
  }
}
`);
    const cmd = Pool.resolveHandlerCommand(handlerPath);
    expect(cmd[1]).toContain('yon-js-runner.js');
    const proc = Bun.spawn({
        cmd,
        stdin: 'pipe',
        stdout: 'pipe',
        stderr: 'pipe',
    });
    proc.stdin.write(JSON.stringify({ method: 'GET', context: { requestId: 'abc' } }));
    proc.stdin.end();
    const [stdout, stderr, exitCode] = await Promise.all([text(proc.stdout), text(proc.stderr), proc.exited]);
    expect(exitCode).toBe(0);
    expect(stderr).toBe('');
    expect(JSON.parse(responseBody(stdout))).toEqual({ message: 'ok', requestId: 'abc' });
});

test('JavaScript handler console output is sidebanded away from the response frame', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-console-sideband-'));
    tempDirs.push(root);
    const handlerPath = path.join(root, 'yon.js');
    await Bun.write(handlerPath, `export class Handler {
  static async GET() {
    console.log('[adapter] doing work')
    return { ok: true }
  }
}
`);
    const cmd = Pool.resolveHandlerCommand(handlerPath);
    const proc = Bun.spawn({
        cmd,
        stdin: 'pipe',
        stdout: 'pipe',
        stderr: 'pipe',
    });
    proc.stdin.write(JSON.stringify({ method: 'GET', context: { requestId: 'abc' } }));
    proc.stdin.end();
    const [stdout, stderr, exitCode] = await Promise.all([text(proc.stdout), text(proc.stderr), proc.exited]);
    expect(exitCode).toBe(0);
    expect(stderr).toContain('[log] [adapter] doing work');
    expect(JSON.parse(responseBody(stdout))).toEqual({ ok: true });
});

test('JavaScript handlers can stream async iterable chunks for SSE requests', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-js-stream-'));
    tempDirs.push(root);
    const handlerPath = path.join(root, 'yon.js');
    await Bun.write(handlerPath, `export class Handler {
  static async *GET() {
    yield ': connected\\n\\n'
    yield 'data: {"message":"one"}\\n\\n'
    yield 'data: {"message":"two"}\\n\\n'
  }
}
`);
    const cmd = Pool.resolveHandlerCommand(handlerPath);
    const proc = Bun.spawn({
        cmd,
        stdin: 'pipe',
        stdout: 'pipe',
        stderr: 'pipe',
    });
    proc.stdin.write(JSON.stringify({
        method: 'GET',
        headers: { accept: 'text/event-stream' },
        context: { requestId: 'abc' },
    }));
    proc.stdin.end();
    const [stdout, stderr, exitCode] = await Promise.all([text(proc.stdout), text(proc.stderr), proc.exited]);
    expect(exitCode).toBe(0);
    expect(stderr).toBe('');
    expect(stdout).not.toContain(Yon.RESPONSE_START);
    expect(stdout).toContain(': connected\n\n');
    expect(stdout).toContain('data: {"message":"one"}\n\n');
    expect(stdout).toContain('data: {"message":"two"}\n\n');
});

test('Yon returns JavaScript handler response before background timers finish', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-bg-response-'));
    tempDirs.push(root);
    const handlerPath = path.join(root, 'yon.js');
    await Bun.write(handlerPath, `export class Handler {
  static async GET() {
    setTimeout(() => console.log('[bg] finished'), 600)
    return { ok: true }
  }
}
`);
    const startedAt = performance.now();
    const response = await Yon.getResponse([handlerPath], { method: 'GET' }, {
        requestId: 'abc',
        ipAddress: '127.0.0.1',
        protocol: 'http',
        host: 'localhost',
    }, undefined);
    const elapsedMs = performance.now() - startedAt;
    expect(response.status).toBe(200);
    expect(JSON.parse(response.body ?? '{}')).toEqual({ ok: true });
    expect(elapsedMs).toBeLessThan(500);
});

test('runs Python class-per-route handlers without user stdin/stdout code', async () => {
    if (!commandAvailable('python3'))
        return;
    const handlerPath = await writeExecutableHandler(`#!/usr/bin/env python3
class Handler:
    @staticmethod
    def GET(request):
        return { "message": "ok", "requestId": request["context"]["requestId"] }
`);
    const cmd = Pool.resolveHandlerCommand(handlerPath);
    expect(cmd[1]).toContain('yon-python-runner.py');
    const proc = Bun.spawn({
        cmd,
        stdin: 'pipe',
        stdout: 'pipe',
        stderr: 'pipe',
    });
    proc.stdin.write(JSON.stringify({ method: 'GET', context: { requestId: 'abc' } }));
    proc.stdin.end();
    const [stdout, stderr, exitCode] = await Promise.all([text(proc.stdout), text(proc.stderr), proc.exited]);
    expect(exitCode).toBe(0);
    expect(stderr).toBe('');
    expect(JSON.parse(stdout)).toEqual({ message: 'ok', requestId: 'abc' });
});

test('runs PHP class-per-route handlers with extensionless shebang route files', async () => {
    if (!commandAvailable('php'))
        return;
    const handlerPath = await writeExecutableHandler(`#!/usr/bin/env php
<?php
class Handler {
    public static function GET($request) {
        return [
            "message" => "ok",
            "requestId" => $request["context"]["requestId"],
        ];
    }
}
`);
    const cmd = Pool.resolveHandlerCommand(handlerPath);
    expect(cmd[1]).toContain('yon-php-runner.php');
    const proc = Bun.spawn({
        cmd,
        stdin: 'pipe',
        stdout: 'pipe',
        stderr: 'pipe',
    });
    proc.stdin.write(JSON.stringify({ method: 'GET', context: { requestId: 'abc' } }));
    proc.stdin.end();
    const [stdout, stderr, exitCode] = await Promise.all([text(proc.stdout), text(proc.stderr), proc.exited]);
    expect(exitCode).toBe(0);
    expect(stderr).toBe('');
    expect(JSON.parse(stdout)).toEqual({ message: 'ok', requestId: 'abc' });
});

test('runs Java class-per-route handlers when javac is available', async () => {
    if (!commandAvailable('javac') || !commandAvailable('java'))
        return;
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-java-handler-'));
    tempDirs.push(root);
    const handlerPath = path.join(root, 'yon.java');
    await Bun.write(handlerPath, `import java.util.LinkedHashMap;
import java.util.Map;

public class Handler {
  public static Object GET(Map<String, Object> request) {
    Map<String, Object> context = (Map<String, Object>) request.get("context");
    Map<String, Object> response = new LinkedHashMap<>();
    response.put("message", "ok");
    response.put("requestId", context.get("requestId"));
    return response;
  }
}
`);
    const output = await YonCompiledRunner.runJava(handlerPath, JSON.stringify({ method: 'GET', context: { requestId: 'abc' } }));
    expect(JSON.parse(output)).toEqual({ message: 'ok', requestId: 'abc' });
});

test('generates Java JSON support for dependency-free compiled handlers', () => {
    const support = YonCompiledRunner.javaJsonSupportSource();
    const main = YonCompiledRunner.javaMainSource();
    expect(support).toContain('public static Object parse(');
    expect(support).toContain('public static String stringify(');
    expect(main).toContain('request.get("method")');
    expect(main).toContain('Handler.class.getDeclaredMethods()');
});

test('runs C++ class-per-route handlers when a compiler is available', async () => {
    if (!commandAvailable('clang++') && !commandAvailable('g++') && !commandAvailable('c++'))
        return;
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-cpp-handler-'));
    tempDirs.push(root);
    const handlerPath = path.join(root, 'yon.cpp');
    await Bun.write(handlerPath, `#include "YonJson.hpp"

class Handler {
public:
  static YonJson GET(const YonJson& request) {
    std::string requestId = "unknown";
    if (const YonJson* context = request.get("context")) {
      if (const YonJson* value = context->get("requestId")) {
        requestId = value->asString("unknown");
      }
    }
    return YonJson::object({
      {"message", "ok"},
      {"requestId", requestId},
    });
  }
};
`);
    const output = await YonCompiledRunner.runCpp(handlerPath, JSON.stringify({ method: 'GET', context: { requestId: 'abc' } }));
    expect(JSON.parse(output)).toEqual({ message: 'ok', requestId: 'abc' });
});

test('generates C++ JSON support for dependency-free compiled handlers', () => {
    const support = YonCompiledRunner.cppJsonSupportSource();
    const main = YonCompiledRunner.cppMainSource(['GET']);
    expect(support).toContain('class YonJson');
    expect(support).toContain('static YonJson parse(');
    expect(main).toContain('Handler::GET(request)');
});

test('runs Rust impl Handler routes when rustc is available', async () => {
    if (!commandAvailable('rustc'))
        return;
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-rust-handler-'));
    tempDirs.push(root);
    const handlerPath = path.join(root, 'yon.rs');
    await Bun.write(handlerPath, `struct Handler;

impl Handler {
    pub fn GET(request: &YonJson) -> YonJson {
        let request_id = request
            .get("context")
            .and_then(|context| context.get("requestId"))
            .map(|value| value.as_string())
            .unwrap_or_else(|| "unknown".to_string());

        YonJson::object(vec![
            ("message", "ok".into()),
            ("requestId", request_id.into()),
        ])
    }
}
`);
    const output = await YonCompiledRunner.runRust(handlerPath, JSON.stringify({ method: 'GET', context: { requestId: 'abc' } }));
    expect(JSON.parse(output)).toEqual({ message: 'ok', requestId: 'abc' });
}, 60000);

test('generates Rust JSON support for dependency-free compiled handlers', () => {
    const support = YonCompiledRunner.rustJsonSupportSource();
    const main = YonCompiledRunner.rustMainSource(['GET'], ['RustLanguageService.rs']);
    expect(support).toContain('pub enum YonJson');
    expect(support).toContain('pub fn parse(');
    expect(support).toContain('pub fn stringify(');
    expect(main).toContain('include!("RustLanguageService.rs");');
    expect(main).toContain('Handler::GET(&request)');
});

test('production Java handlers compile once and reuse the artifact across methods', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-java-cache-'));
    tempDirs.push(root);
    const handlerPath = path.join(root, 'yon.java');
    await Bun.write(handlerPath, `import java.util.Map;

public class Handler {
  public static Object GET(Map<String, Object> request) {
    return "{\\"method\\":\\"GET\\"}";
  }

  public static Object POST(Map<String, Object> request) {
    return "{\\"method\\":\\"POST\\"}";
  }
}
`);
    /** @type {string[]} */
    const commands = [];
    await withProductionEnvironment(async () => {
        tempDirs.push(YonCompiledRunner.workspace('java', handlerPath));
        await withRunCommandSpy(async (cmd, _cwd, input) => {
            commands.push(cmd.join(' '));
            if (cmd[0] === 'java')
                return input?.includes('"POST"') ? '{"method":"POST"}' : '{"method":"GET"}';
            return '';
        }, async () => {
            expect(await YonCompiledRunner.runJava(handlerPath, JSON.stringify({ method: 'GET' }))).toBe('{"method":"GET"}');
            expect(await YonCompiledRunner.runJava(handlerPath, JSON.stringify({ method: 'POST' }))).toBe('{"method":"POST"}');
        });
    });
    expect(commands.filter((command) => command.startsWith('javac '))).toHaveLength(1);
    expect(commands.filter((command) => command.startsWith('java '))).toHaveLength(2);
});

test('production C# handlers publish once and reuse the artifact across methods', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-csharp-cache-'));
    tempDirs.push(root);
    const handlerPath = path.join(root, 'yon.cs');
    await Bun.write(handlerPath, `using System.Text.Json;

public class Handler {
    public static object GET(JsonElement request) => new { method = "GET" };
    public static object POST(JsonElement request) => new { method = "POST" };
}
`);
    /** @type {string[]} */
    const commands = [];
    await withProductionEnvironment(async () => {
        tempDirs.push(YonCompiledRunner.workspace('csharp', handlerPath));
        await withRunCommandSpy(async (cmd, _cwd, input) => {
            commands.push(cmd.join(' '));
            if (cmd[0] === 'dotnet' && cmd[1]?.endsWith('YonRoute.dll'))
                return input?.includes('"POST"') ? '{"method":"POST"}' : '{"method":"GET"}';
            return '';
        }, async () => {
            expect(await YonCompiledRunner.runCSharp(handlerPath, JSON.stringify({ method: 'GET' }))).toBe('{"method":"GET"}');
            expect(await YonCompiledRunner.runCSharp(handlerPath, JSON.stringify({ method: 'POST' }))).toBe('{"method":"POST"}');
        });
    });
    expect(commands.filter((command) => command.startsWith('dotnet publish '))).toHaveLength(1);
    expect(commands.filter((command) => command.includes('YonRoute.dll'))).toHaveLength(2);
});

test('production Dart handlers compile once and reuse the kernel across methods', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-dart-cache-'));
    tempDirs.push(root);
    const handlerPath = path.join(root, 'yon.dart');
    await Bun.write(handlerPath, `class Handler {
  static Map<String, Object?> GET(Map<String, dynamic> request) => {'method': 'GET'};
  static Map<String, Object?> POST(Map<String, dynamic> request) => {'method': 'POST'};
}
`);
    /** @type {string[]} */
    const commands = [];
    await withProductionEnvironment(async () => {
        tempDirs.push(YonCompiledRunner.workspace('dart', handlerPath));
        await withRunCommandSpy(async (cmd, _cwd, input) => {
            commands.push(cmd.join(' '));
            if (cmd.includes('main.dill') && cmd[1] === 'run')
                return input?.includes('"POST"') ? '{"method":"POST"}' : '{"method":"GET"}';
            return '';
        }, async () => {
            expect(await YonCompiledRunner.runDart(handlerPath, JSON.stringify({ method: 'GET' }))).toBe('{"method":"GET"}');
            expect(await YonCompiledRunner.runDart(handlerPath, JSON.stringify({ method: 'POST' }))).toBe('{"method":"POST"}');
        });
    });
    expect(commands.filter((command) => command.includes('compile kernel'))).toHaveLength(1);
    expect(commands.filter((command) => command.includes('run main.dill'))).toHaveLength(2);
});

test('production C++ handlers compile once and reuse the binary across methods', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-cpp-cache-'));
    tempDirs.push(root);
    const handlerPath = path.join(root, 'yon.cpp');
    await Bun.write(handlerPath, `#include "YonJson.hpp"

class Handler {
public:
  static YonJson GET(const YonJson& request) {
    return YonJson::object({{"method", "GET"}});
  }

  static YonJson POST(const YonJson& request) {
    return YonJson::object({{"method", "POST"}});
  }
};
`);
    /** @type {string[]} */
    const commands = [];
    await withProductionEnvironment(async () => {
        tempDirs.push(YonCompiledRunner.workspace('cpp', handlerPath));
        await withRunCommandSpy(async (cmd, _cwd, input) => {
            commands.push(cmd.join(' '));
            if (cmd[0]?.endsWith('handler') || cmd[0]?.endsWith('handler.exe'))
                return input?.includes('"POST"') ? '{"method":"POST"}' : '{"method":"GET"}';
            return '';
        }, async () => {
            expect(await YonCompiledRunner.runCpp(handlerPath, JSON.stringify({ method: 'GET' }))).toBe('{"method":"GET"}');
            expect(await YonCompiledRunner.runCpp(handlerPath, JSON.stringify({ method: 'POST' }))).toBe('{"method":"POST"}');
        });
    });
    expect(commands.filter((command) => command.includes(' -std=c++17 '))).toHaveLength(1);
    expect(commands.filter((command) => command.endsWith('/handler') || command.endsWith('\\handler.exe'))).toHaveLength(2);
});

test('production Rust handlers compile once and reuse the binary across methods', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-rust-cache-'));
    tempDirs.push(root);
    const handlerPath = path.join(root, 'yon.rs');
    await Bun.write(handlerPath, `struct Handler;

impl Handler {
    pub fn GET(_request: &YonJson) -> YonJson {
        YonJson::object(vec![("method", "GET".into())])
    }

    pub fn POST(_request: &YonJson) -> YonJson {
        YonJson::object(vec![("method", "POST".into())])
    }
}
`);
    /** @type {string[]} */
    const commands = [];
    await withProductionEnvironment(async () => {
        tempDirs.push(YonCompiledRunner.workspace('rust', handlerPath));
        await withRunCommandSpy(async (cmd, _cwd, input) => {
            commands.push(cmd.join(' '));
            if (cmd[0]?.endsWith('handler') || cmd[0]?.endsWith('handler.exe'))
                return input?.includes('"POST"') ? '{"method":"POST"}' : '{"method":"GET"}';
            return '';
        }, async () => {
            expect(await YonCompiledRunner.runRust(handlerPath, JSON.stringify({ method: 'GET' }))).toBe('{"method":"GET"}');
            expect(await YonCompiledRunner.runRust(handlerPath, JSON.stringify({ method: 'POST' }))).toBe('{"method":"POST"}');
        });
    });
    expect(commands.filter((command) => command.startsWith('rustc '))).toHaveLength(1);
    expect(commands.filter((command) => command.endsWith('/handler') || command.endsWith('\\handler.exe'))).toHaveLength(2);
});

test('custom language providers register and resolve handlers', async () => {
    HandlerAdapter.registerProvider({
        language: 'lua',
        extensions: ['.lua'],
        shebangs: ['lua*'],
        command: (handler) => ['lua', handler],
        hasHandlerClass: (source) => /\bHandler\s*=/.test(source),
        hasMethod: (source, method) => new RegExp(`\\bfunction\\s+Handler\\.${method}\\b`).test(source),
    });
    expect(HandlerAdapter.knownLanguages).toContain('lua');

    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-lua-provider-'));
    tempDirs.push(root);
    const handlerPath = path.join(root, 'yon.lua');
    await Bun.write(handlerPath, `Handler = {}
function Handler.GET(request)
    return { ok = true }
end
`);
    const adapter = HandlerAdapter.resolve(handlerPath, []);
    expect(adapter).not.toBeNull();
    expect(adapter?.language).toBe('lua');
    expect(adapter?.command).toEqual(['lua', handlerPath]);
    expect([...(adapter?.methods ?? [])]).toEqual(['GET']);
});

test('a prebuilt executable handler resolves via OPTIONS.schema.json', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-generic-exec-'));
    tempDirs.push(root);
    // An extension with no interpreter mapping — a self-runnable binary or
    // shebang script. Runs directly, no extension→interpreter lookup.
    const handlerPath = path.join(root, 'yon.zig-binary');
    await Bun.write(handlerPath, '#!/bin/sh\nread body\necho "{\\"ok\\":true}"\n');
    await chmod(handlerPath, 0o755);

    // Without the sidecar the handler is rejected — methods are unknowable.
    expect(HandlerAdapter.resolve(handlerPath, [])).toBeNull();

    await Bun.write(path.join(root, 'OPTIONS.schema.json'), JSON.stringify({
        GET: { response: { ok: '^true$' } },
        POST: { payload: { body: '^[\\s\\S]*$' } },
    }));
    const adapter = HandlerAdapter.resolve(handlerPath, []);
    expect(adapter).not.toBeNull();
    expect(adapter?.language).toBe('executable');
    expect(adapter?.command).toEqual([handlerPath]);
    expect([...(adapter?.methods ?? [])].sort()).toEqual(['GET', 'POST']);

    // The spawned executable speaks the stdin/stdout protocol as-is.
    const proc = Bun.spawn({ cmd: adapter?.command ?? [], stdin: 'pipe', stdout: 'pipe' });
    proc.stdin.write(JSON.stringify({ method: 'GET' }));
    proc.stdin.end();
    expect((await new Response(proc.stdout).text()).trim()).toBe('{"ok":true}');
});

test('any language runs by extension — no shebang, no chmod', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-by-ext-'));
    tempDirs.push(root);
    // A plain, non-executable yon.sh — no shebang, never chmod'd. Its
    // extension resolves to the `sh` interpreter (a default), so Yon runs it.
    const handlerPath = path.join(root, 'yon.sh');
    await Bun.write(handlerPath, 'read body\necho "{\\"engine\\":\\"sh\\"}"\n');
    await Bun.write(path.join(root, 'OPTIONS.schema.json'), JSON.stringify({ POST: {} }));

    const adapter = HandlerAdapter.resolve(handlerPath, []);
    expect(adapter).not.toBeNull();
    expect(adapter?.command).toEqual(['sh', handlerPath]);
    expect([...(adapter?.methods ?? [])]).toEqual(['POST']);

    const proc = Bun.spawn({ cmd: adapter?.command ?? [], stdin: 'pipe', stdout: 'pipe' });
    proc.stdin.write(JSON.stringify({ method: 'POST', body: 'x' }));
    proc.stdin.end();
    expect((await new Response(proc.stdout).text()).trim()).toBe('{"engine":"sh"}');
});

test('a known-language file may opt into the raw protocol via the universal path', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-raw-py-'));
    tempDirs.push(root);
    // A .py handler (python HAS an adapter) that does NOT use class Handler —
    // it speaks the protocol itself, so it falls through to the universal
    // path and runs via the .py interpreter (python3), no shebang needed.
    const handlerPath = path.join(root, 'yon.py');
    await Bun.write(handlerPath, 'import sys, json\nsys.stdin.read()\nprint(json.dumps({"raw": True}))\n');
    await Bun.write(path.join(root, 'OPTIONS.schema.json'), JSON.stringify({ GET: {} }));

    const adapter = HandlerAdapter.resolve(handlerPath, []);
    expect(adapter).not.toBeNull();
    expect(adapter?.command).toEqual(['python3', handlerPath]);
    expect([...(adapter?.methods ?? [])]).toEqual(['GET']);
});
