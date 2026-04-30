// @ts-check
import { afterEach, expect, test } from 'bun:test';
import { chmod, mkdtemp, rm } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import HandlerAdapter from '../../src/server/process/handler-adapter.js';
import Pool from '../../src/server/process/process-pool.js';
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

test('documents the top-10 Yon handler language targets', () => {
    expect(HandlerAdapter.supportedLanguages).toEqual([
        'javascript',
        'typescript',
        'python',
        'ruby',
        'php',
        'dart',
        'go',
        'java',
        'csharp',
        'rust',
    ]);
});

test('runs JavaScript pure-function handlers without user stdin/stdout code', async () => {
    const handlerPath = await writeExecutableHandler(`#!/usr/bin/env bun
export async function handler(request) {
  return { message: 'ok', requestId: request.context.requestId }
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
    proc.stdin.write(JSON.stringify({ context: { requestId: 'abc' } }));
    proc.stdin.end();
    const [stdout, stderr, exitCode] = await Promise.all([text(proc.stdout), text(proc.stderr), proc.exited]);
    expect(exitCode).toBe(0);
    expect(stderr).toBe('');
    expect(JSON.parse(stdout)).toEqual({ message: 'ok', requestId: 'abc' });
});

test('resolves extensioned route modules without shebangs', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-extension-adapter-'));
    tempDirs.push(root);
    const handlerPath = path.join(root, 'GET.js');
    await Bun.write(handlerPath, `export async function handler(request) {
  return { message: 'ok', requestId: request.context.requestId }
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
    proc.stdin.write(JSON.stringify({ context: { requestId: 'abc' } }));
    proc.stdin.end();
    const [stdout, stderr, exitCode] = await Promise.all([text(proc.stdout), text(proc.stderr), proc.exited]);
    expect(exitCode).toBe(0);
    expect(stderr).toBe('');
    expect(JSON.parse(stdout)).toEqual({ message: 'ok', requestId: 'abc' });
});

test('runs Python class handlers without user stdin/stdout code', async () => {
    if (!commandAvailable('python3'))
        return;
    const handlerPath = await writeExecutableHandler(`#!/usr/bin/env python3
class GET:
    def handler(self, request):
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
    proc.stdin.write(JSON.stringify({ context: { requestId: 'abc' } }));
    proc.stdin.end();
    const [stdout, stderr, exitCode] = await Promise.all([text(proc.stdout), text(proc.stderr), proc.exited]);
    expect(exitCode).toBe(0);
    expect(stderr).toBe('');
    expect(JSON.parse(stdout)).toEqual({ message: 'ok', requestId: 'abc' });
});

test('runs PHP handlers with extensionless shebang route files', async () => {
    if (!commandAvailable('php'))
        return;
    const handlerPath = await writeExecutableHandler(`#!/usr/bin/env php
<?php
function handler($request) {
    return [
        "message" => "ok",
        "requestId" => $request["context"]["requestId"],
    ];
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
    proc.stdin.write(JSON.stringify({ context: { requestId: 'abc' } }));
    proc.stdin.end();
    const [stdout, stderr, exitCode] = await Promise.all([text(proc.stdout), text(proc.stderr), proc.exited]);
    expect(exitCode).toBe(0);
    expect(stderr).toBe('');
    expect(JSON.parse(stdout)).toEqual({ message: 'ok', requestId: 'abc' });
});

test('runs Java handlers with a dependency-free Tachyon request object when javac is available', async () => {
    if (!commandAvailable('javac') || !commandAvailable('java'))
        return;
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-java-handler-'));
    tempDirs.push(root);
    const handlerPath = path.join(root, 'POST.java');
    await Bun.write(handlerPath, `import java.util.LinkedHashMap;
import java.util.Map;

public class POST {
  public static Object handler(Map<String, Object> request) {
    Map<String, Object> context = (Map<String, Object>) request.get("context");
    Map<String, Object> response = new LinkedHashMap<>();
    response.put("message", "ok");
    response.put("requestId", context.get("requestId"));
    return response;
  }
}
`);
    const output = await YonCompiledRunner.runJava(handlerPath, JSON.stringify({ context: { requestId: 'abc' } }));
    expect(JSON.parse(output)).toEqual({ message: 'ok', requestId: 'abc' });
});

test('generates Rust JSON support for dependency-free compiled handlers', () => {
    const support = YonCompiledRunner.rustJsonSupportSource();
    const main = YonCompiledRunner.rustMainSource();
    expect(support).toContain('pub enum JsonValue');
    expect(support).toContain('pub fn parse(input: &str)');
    expect(support).toContain('pub fn stringify(value: &JsonValue)');
    expect(main).toContain('let request = yon_json::parse(&request_json)');
    expect(main).toContain('user::handler(&request)');
});
