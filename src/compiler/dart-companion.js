// @ts-check
/**
 * Compiles a Dart Tac companion to browser JavaScript and wraps it in the
 * controller shape consumed by Tac templates. Dart remains a source language;
 * the generated bridge is an implementation detail of the bundle.
 */

import { mkdir, mkdtemp, readFile, rename, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import TachyonRuntimeCache from '../shared/runtime-cache.js';
import DartToolchain from './dart-toolchain.js';

/** @typedef {{ name: string, initial?: unknown }} DartMember */
/** @typedef {{ field: string, name: string }} DartPublishedField */
/** @typedef {{ method: string, name: string }} DartSubscription */

/**
 * The intentionally small, import-free Dart syntax Tac owns. Parsing only the
 * public controller surface lets the generated JavaScript bridge remain static
 * and avoids unsupported reflection in Dart's web compiler.
 */
export class DartCompanionContract {
    /** @type {string} */
    className;
    /** @type {DartMember[]} */
    fields;
    /** @type {DartMember[]} */
    methods;
    /** @type {DartPublishedField[]} */
    publishedFields;
    /** @type {string[]} */
    mountMethods;
    /** @type {DartSubscription[]} */
    subscriptions;

    /**
     * @param {{ className: string, fields: DartMember[], methods: DartMember[], publishedFields: DartPublishedField[], mountMethods: string[], subscriptions: DartSubscription[] }} options
     */
    constructor(options) {
        this.className = options.className;
        this.fields = options.fields;
        this.methods = options.methods;
        this.publishedFields = options.publishedFields;
        this.mountMethods = options.mountMethods;
        this.subscriptions = options.subscriptions;
    }

    /**
     * @param {string} source
     * @param {string} sourcePath
     * @returns {DartCompanionContract}
     */
    static parse(source, sourcePath) {
        const masked = DartCompanionContract.mask(source);
        if (/\b(?:Web|App|Clipboard|Fylo|FilePicker)\.|\blaunchUrl\s*\(/.test(masked))
            throw new Error(`Dart Tac companion '${sourcePath}' uses a removed platform wrapper. Use the implicit language prelude instead.`);
        if (/^\s*(?:import|export|library|part)\b/m.test(masked)) {
            throw new Error(`Dart Tac companion '${sourcePath}' must not declare imports, exports, libraries, or parts. Tac supplies its runtime APIs.`);
        }

        const matches = [...masked.matchAll(/(?:^|\n)\s*class\s+([A-Za-z_$][\w$]*)\s+extends\s+Tac\s*\{/g)];
        if (matches.length !== 1) {
            throw new Error(`Dart Tac companion '${sourcePath}' must declare exactly one 'class Name extends Tac'.`);
        }

        const match = matches[0];
        const className = match[1];
        const openBrace = /** @type {number} */ (match.index) + match[0].lastIndexOf('{');
        const closeBrace = DartCompanionContract.findMatchingBrace(masked, openBrace);
        if (closeBrace === -1)
            throw new Error(`Dart Tac companion '${sourcePath}' has an unclosed class body.`);

        const body = source.slice(openBrace + 1, closeBrace);
        const maskedBody = masked.slice(openBrace + 1, closeBrace);
        /** @type {DartMember[]} */
        const fields = [];
        /** @type {DartMember[]} */
        const methods = [];
        /** @type {DartPublishedField[]} */
        const publishedFields = [];
        /** @type {string[]} */
        const mountMethods = [];
        /** @type {DartSubscription[]} */
        const subscriptions = [];

        for (const member of DartCompanionContract.topLevelMembers(body, maskedBody)) {
            const annotations = DartCompanionContract.annotations(member.source);
            const declaration = DartCompanionContract.removeAnnotations(member.masked).trim();
            if (!declaration)
                continue;

            if (member.kind === 'method') {
                const methodName = DartCompanionContract.methodName(declaration);
                if (!methodName)
                    continue;
                if (methodName === className) {
                    throw new Error(`Dart Tac companion '${sourcePath}' must use the implicit constructor so Tac can provide props and runtime bindings.`);
                }
                if (!methodName.startsWith('_')) {
                    methods.push({ name: methodName });
                    const subscription = annotations.find((annotation) => annotation.name === 'subscribe');
                    if (subscription)
                        subscriptions.push({ method: methodName, name: subscription.value || methodName });
                    if (annotations.some((annotation) => annotation.name === 'onMount'))
                        mountMethods.push(methodName);
                }
                continue;
            }

            if (new RegExp(`^${className}\\s*\\(`).test(declaration)) {
                throw new Error(`Dart Tac companion '${sourcePath}' must use the implicit constructor so Tac can provide props and runtime bindings.`);
            }
            const field = DartCompanionContract.fieldDefinition(declaration, DartCompanionContract.removeAnnotations(member.source));
            if (!field || field.name.startsWith('_') || field.name === 'props' || field.name === 'tac')
                continue;
            fields.push(field);
            const publication = annotations.find((annotation) => annotation.name === 'publish');
            if (publication)
                publishedFields.push({ field: field.name, name: publication.value || field.name });
        }

        return new DartCompanionContract({
            className,
            fields: DartCompanionContract.uniqueMembers(fields),
            methods: DartCompanionContract.uniqueMembers(methods),
            publishedFields,
            mountMethods: [...new Set(mountMethods)],
            subscriptions,
        });
    }

    /** @param {DartMember[]} members */
    static uniqueMembers(members) {
        return [...new Map(members.map((member) => [member.name, member])).values()];
    }

    /** @param {string} source */
    static mask(source) {
        return source
            .replace(/\/\*[\s\S]*?\*\//g, (value) => value.replace(/[^\n]/g, ' '))
            .replace(/\/\/.*$/gm, (value) => ' '.repeat(value.length))
            .replace(/(['"])(?:\\.|(?!\1)[\s\S])*?\1/g, (value) => value.replace(/[^\n]/g, ' '));
    }

    /** @param {string} source @param {number} openBrace */
    static findMatchingBrace(source, openBrace) {
        let depth = 0;
        for (let index = openBrace; index < source.length; index += 1) {
            if (source[index] === '{') depth += 1;
            else if (source[index] === '}') {
                depth -= 1;
                if (depth === 0) return index;
            }
        }
        return -1;
    }

    /**
     * @param {string} source
     * @param {string} masked
     * @returns {Array<{ kind: 'field' | 'method', source: string, masked: string }>}
     */
    static topLevelMembers(source, masked) {
        /** @type {Array<{ kind: 'field' | 'method', source: string, masked: string }>} */
        const members = [];
        let start = 0;
        let depth = 0;
        for (let index = 0; index < masked.length; index += 1) {
            const character = masked[index];
            if (character === '{') {
                if (depth === 0) {
                    const end = DartCompanionContract.findMatchingBrace(masked, index);
                    if (end === -1) break;
                    members.push({ kind: 'method', source: source.slice(start, end + 1), masked: masked.slice(start, end + 1) });
                    start = end + 1;
                    index = end;
                    continue;
                }
                depth += 1;
                continue;
            }
            if (character === '}') {
                depth = Math.max(0, depth - 1);
                continue;
            }
            if (character === ';' && depth === 0) {
                members.push({ kind: 'field', source: source.slice(start, index + 1), masked: masked.slice(start, index + 1) });
                start = index + 1;
            }
        }
        return members;
    }

    /** @param {string} source */
    static annotations(source) {
        /** @type {Array<{ name: string, value: string }>} */
        const annotations = [];
        for (const match of source.matchAll(/@([A-Za-z_$][\w$]*)(?:\s*\(\s*(['"])(.*?)\2\s*\))?/g)) {
            annotations.push({ name: match[1], value: match[3] ?? '' });
        }
        return annotations;
    }

    /** @param {string} source */
    static removeAnnotations(source) {
        return source.replace(/@([A-Za-z_$][\w$]*)(?:\s*\([^)]*\))?/g, ' ');
    }

    /** @param {string} declaration */
    static methodName(declaration) {
        const signature = declaration.slice(0, declaration.indexOf('{'));
        const matches = [...signature.matchAll(/([A-Za-z_$][\w$]*)\s*\(/g)];
        return matches.at(-1)?.[1] ?? null;
    }

    /** @param {string} declaration @param {string} source */
    static fieldDefinition(declaration, source) {
        if (/\b(?:static|class)\b/.test(declaration) || declaration.includes('=>'))
            return null;
        const match = declaration.match(/(?:^|\s)([A-Za-z_$][\w$]*)\s*(?:=[\s\S]*)?;$/);
        if (!match)
            return null;
        const name = match[1];
        const initialMatch = source.match(new RegExp(`\\b${name.replace(/[$]/g, '\\$&')}\\s*=\\s*([\\s\\S]*);\\s*$`));
        const initial = initialMatch ? DartCompanionContract.literalValue(initialMatch[1].trim()) : undefined;
        return initial === undefined ? { name } : { name, initial };
    }

    /** @param {string} source */
    static literalValue(source) {
        if (source === 'null') return null;
        if (source === 'true') return true;
        if (source === 'false') return false;
        if (/^-?(?:0|[1-9]\d*)(?:\.\d+)?$/.test(source)) return Number(source);
        if (/^"(?:\\.|[^"\\])*"$/.test(source)) {
            try { return JSON.parse(source); } catch { return undefined; }
        }
        if (/^'(?:\\.|[^'\\])*'$/.test(source))
            return source.slice(1, -1).replaceAll("\\'", "'").replaceAll('\\\\', '\\');
        if (/^\[(?:[\s\S]*)\]$/.test(source)) {
            try { return JSON.parse(source); } catch { return undefined; }
        }
        return undefined;
    }
}

/**
 * Runs the Dart JavaScript compiler and appends the JavaScript controller
 * adapter. A managed SDK is provisioned into Tachyon's cache when a host SDK
 * is absent, while TACHYON_DART_COMPILER remains an explicit override.
 */
export default class DartCompanionCompiler {
    /** @param {string} [command] @param {DartToolchain} [toolchain] */
    constructor(command = process.env.TACHYON_DART_COMPILER || process.env.DART || '', toolchain = new DartToolchain()) {
        this.command = command;
        this.toolchain = toolchain;
    }

    /** @returns {Promise<string>} */
    async resolveCommand() {
        if (this.command) {
            if (path.isAbsolute(this.command) && await Bun.file(this.command).exists())
                return this.command;
            const resolved = Bun.which(this.command);
            if (resolved)
                return resolved;
            throw new Error(`TACHYON_DART_COMPILER could not be found: ${this.command}`);
        }
        const hostCommand = Bun.which('dart');
        return hostCommand || await this.toolchain.ensure();
    }

    /** @type {Map<string, string>} resolved compiler path → `dart --version` output */
    static versions = new Map();
    /** @type {Map<string, string>} content-address → compiled runtime JavaScript */
    static compiled = new Map();
    static cacheDir = path.join(TachyonRuntimeCache.cacheRoot(), 'dart-companions');

    /** @param {string} command */
    async compilerVersion(command) {
        let version = DartCompanionCompiler.versions.get(command);
        if (version === undefined) {
            const probe = Bun.spawn({ cmd: [command, '--version'], stdout: 'pipe', stderr: 'pipe' });
            const [stdout, stderr] = await Promise.all([
                new Response(probe.stdout).text(),
                new Response(probe.stderr).text(),
                probe.exited,
            ]);
            version = (stdout.trim() || stderr.trim());
            DartCompanionCompiler.versions.set(command, version);
        }
        return version;
    }

    /**
     * Compiled output is content-addressed by the generated Dart entry plus
     * the compiler version, so watch rebuilds, repeat bundles, and fresh
     * processes reuse prior `dart compile js` runs instead of paying its
     * multi-second cost again. A source edit changes the key, so output can
     * never go stale.
     * @param {string} sourcePath
     * @returns {Promise<{ code: string, runtimeCode: string, contract: DartCompanionContract, factoryName: string }>}
     */
    async compile(sourcePath) {
        const source = await readFile(sourcePath, 'utf8');
        const contract = DartCompanionContract.parse(source, sourcePath);
        const command = await this.resolveCommand();
        const factoryName = `__tcDartCompanion_${Bun.hash(sourcePath).toString(36).replace('-', 'n')}`;
        const entrySource = this.createDartSource(source, contract, factoryName);
        const cacheKey = Bun.hash(`${await this.compilerVersion(command)}\n${entrySource}`).toString(16);
        let runtimeCode = DartCompanionCompiler.compiled.get(cacheKey) ?? await this.readCachedRuntime(cacheKey);
        if (runtimeCode === undefined) {
            runtimeCode = await this.compileEntry(command, entrySource, sourcePath);
            await this.writeCachedRuntime(cacheKey, runtimeCode).catch(() => {});
        }
        DartCompanionCompiler.compiled.set(cacheKey, runtimeCode);
        return {
            code: `${runtimeCode}\n${this.createJavaScriptAdapter(contract, factoryName)}`,
            runtimeCode,
            contract,
            factoryName,
        };
    }

    /** @param {string} cacheKey @returns {Promise<string | undefined>} */
    async readCachedRuntime(cacheKey) {
        try {
            return await readFile(path.join(DartCompanionCompiler.cacheDir, `${cacheKey}.js`), 'utf8');
        }
        catch {
            return undefined;
        }
    }

    /** @param {string} cacheKey @param {string} runtimeCode */
    async writeCachedRuntime(cacheKey, runtimeCode) {
        await mkdir(DartCompanionCompiler.cacheDir, { recursive: true });
        const targetPath = path.join(DartCompanionCompiler.cacheDir, `${cacheKey}.js`);
        const stagingPath = `${targetPath}.${process.pid}.tmp`;
        await writeFile(stagingPath, runtimeCode);
        await rename(stagingPath, targetPath);
    }

    /**
     * @param {string} command
     * @param {string} entrySource
     * @param {string} sourcePath
     * @returns {Promise<string>}
     */
    async compileEntry(command, entrySource, sourcePath) {
        const temporaryRoot = await mkdtemp(path.join(tmpdir(), 'tachyon-dart-companion-'));
        const entryPath = path.join(temporaryRoot, 'main.dart');
        const outputPath = path.join(temporaryRoot, 'tac.dart.js');
        try {
            await writeFile(entryPath, entrySource);
            const process = Bun.spawn({
                cmd: [command, 'compile', 'js', '-O2', '--csp', '--no-source-maps', '-o', outputPath, entryPath],
                stdout: 'pipe',
                stderr: 'pipe',
            });
            const [stdout, stderr, exitCode] = await Promise.all([
                new Response(process.stdout).text(),
                new Response(process.stderr).text(),
                process.exited,
            ]);
            if (exitCode !== 0) {
                throw new Error(`Dart failed to compile '${sourcePath}':\n${stderr || stdout}`.trim());
            }
            return await readFile(outputPath, 'utf8');
        }
        finally {
            await rm(temporaryRoot, { recursive: true, force: true });
        }
    }

    /**
     * @param {string} source
     * @param {DartCompanionContract} contract
     * @param {string} factoryName
     */
    createDartSource(source, contract, factoryName) {
        const readCases = contract.fields
            .map(({ name }) => `case ${JSON.stringify(name)}: return instance.${name};`)
            .join('\n        ');
        const writeCases = contract.fields
            .map(({ name }) => `case ${JSON.stringify(name)}: instance.${name} = value; return;`)
            .join('\n        ');
        const callCases = contract.methods
            .map(({ name }) => `case ${JSON.stringify(name)}: result = Function.apply(instance.${name}, arguments); break;`)
            .join('\n        ');
        return `import 'dart:async';
import 'dart:convert';
import 'dart:js';

class Tac {
  dynamic props;
  dynamic tac;
  Tac([this.props = const {}, this.tac]);

  dynamic publish(String name, [dynamic value]) => _callTac(tac, 'publish', [name, value]);
  dynamic env(String key, [dynamic fallback]) => _callTac(tac, 'env', [key, fallback]);
  dynamic rerender() => _callTac(tac, 'rerender', const []);
}

class subscribe {
  final String? name;
  const subscribe([this.name]);
}

class publish {
  final String? name;
  const publish([this.name]);
}

class onMount {
  const onMount();
}

dynamic _callTac(dynamic runtime, String method, List<dynamic> arguments) {
  if (runtime == null) return null;
  final bridge = runtime is JsObject ? runtime : JsObject.fromBrowserObject(runtime);
  return bridge.callMethod(method, JsArray.from(arguments));
}

// Import-free language prelude. It compiles into Tachyon's private capability
// bridge while keeping app source free of Tac internals and framework names.
dynamic __tcRuntime;
Future<dynamic> _nativeCall(String operation, [dynamic payload]) async {
  final completer = Completer<dynamic>();
  final encoded = payload == null ? null : jsonEncode(payload);
  _callTac(__tcRuntime, '__nativeCallback', [
    operation,
    encoded,
    allowInterop((dynamic value) => completer.complete(value)),
    allowInterop((dynamic error) => completer.completeError(error)),
  ]);
  return await completer.future;
}

class _Clipboard {
  const _Clipboard();
  Future<void> writeText(String text) async => await _nativeCall('clipboard.writeText', { 'text': text });
  Future<String?> readText() async {
    final value = await _nativeCall('clipboard.readText');
    return value == null ? null : value.toString();
  }
}

class _FileSystem {
  const _FileSystem();
  Future<dynamic> readText(String path) => _nativeCall('fs.readText', { 'path': path });
  Future<dynamic> writeText(String path, String text) => _nativeCall('fs.writeText', { 'path': path, 'text': text });
  Future<dynamic> readDir(String path) => _nativeCall('fs.readDir', { 'path': path });
  Future<dynamic> paths() => _nativeCall('fs.paths');
}

class _Shell {
  const _Shell();
  Future<dynamic> exec(String command, [List<String> args = const [], String? cwd]) => _nativeCall(
    'shell.exec',
    { 'command': command, 'args': args, 'cwd': cwd },
  );
}

class _App {
  const _App();
  bool isAvailable() => _callTac(__tcRuntime, '__nativeAvailable', ['app.info']) == true;
  Future<dynamic> info() => _nativeCall('app.info');
}

class _WebStorage {
  final String _scope;
  const _WebStorage(this._scope);
  Future<dynamic> getItem(String key, [dynamic fallback]) => _nativeCall('web.' + _scope + '.getItem', { 'key': key, 'fallback': fallback });
  Future<void> setItem(String key, dynamic value) async => await _nativeCall('web.' + _scope + '.setItem', { 'key': key, 'value': value });
  Future<void> removeItem(String key) async => await _nativeCall('web.' + _scope + '.removeItem', { 'key': key });
}

class _WebNavigator {
  const _WebNavigator();
  Future<String> language() async => String.fromCharCodes((await _nativeCall('web.navigator.language')).toString().codeUnits);
  Future<bool> isOnline() async => (await _nativeCall('web.navigator.online')) == true;
}

class _WebLocation {
  const _WebLocation();
  Future<String> href() async => (await _nativeCall('web.location.href')).toString();
  Future<String> origin() async => (await _nativeCall('web.location.origin')).toString();
}

class _Capabilities {
  const _Capabilities();
  Future<bool> supports(String capability) async => (await _nativeCall('capabilities.supports', { 'capability': capability })) == true;
  Future<String> state(String capability) async => (await _nativeCall('capabilities.state', { 'capability': capability })).toString();
}

class _Fylo {
  const _Fylo();
  _FyloCollection collection(String name) => _FyloCollection(name);
}

class _FyloCollection {
  final String _name;
  const _FyloCollection(this._name);

  Future<dynamic> _call(String method, [List<dynamic> args = const []]) => _nativeCall(
    'fylo.collection.' + method,
    { 'collection': _name, 'args': args },
  );

  Future<dynamic> find([dynamic query = const {}]) => _call('find', [query]);
  Future<dynamic> get(dynamic id) => _call('get', [id]);
  Future<dynamic> create(dynamic document) => _call('create', [document]);
  Future<dynamic> patch(dynamic id, dynamic document) => _call('patch', [id, document]);
  Future<dynamic> delete(dynamic id) => _call('delete', [id]);
  Future<dynamic> list([dynamic limit = 25]) => _call('list', [limit]);
  Future<dynamic> put(dynamic id, dynamic document) => _call('put', [id, document]);
  Future<dynamic> restore(dynamic id) => _call('restore', [id]);
  Future<dynamic> latest(dynamic id) => _call('latest', [id]);
  Future<dynamic> inspect() => _call('inspect');
  Future<dynamic> rebuild() => _call('rebuild');
}

class _Browser {
  const _Browser();
  Future<dynamic> open(dynamic uri) => _nativeCall('browser.open', { 'url': uri.toString() });
}

class _Share {
  const _Share();
  Future<dynamic> text(String text, [String title = '']) => _nativeCall('share.text', { 'text': text, 'title': title });
}

class _Haptics {
  const _Haptics();
  Future<dynamic> impact() => _nativeCall('haptics.impact');
}

class _FilePicker {
  const _FilePicker();
  Future<dynamic> openText() => _nativeCall('filePicker.openText');
  Future<dynamic> saveText(String name, String text) => _nativeCall('filePicker.saveText', { 'name': name, 'text': text });
}

class _Secrets {
  const _Secrets();
  Future<String?> get(String key) async { final value = await _nativeCall('secrets.get', { 'key': key }); return value?.toString(); }
  Future<void> set(String key, String value) async => await _nativeCall('secrets.set', { 'key': key, 'value': value });
  Future<void> delete(String key) async => await _nativeCall('secrets.delete', { 'key': key });
}

class _Auth { const _Auth(); Future<dynamic> verifyUser(String reason) => _nativeCall('auth.verifyUser', { 'reason': reason }); }
class _Geolocation { const _Geolocation(); Future<dynamic> current([dynamic options]) => _nativeCall('geo.current', { 'options': options }); }
class _Notifications { const _Notifications(); Future<dynamic> show(String title, [dynamic options]) => _nativeCall('notify.show', { 'title': title, 'options': options }); }
class _Media { const _Media(); Future<dynamic> getUserMedia(dynamic constraints) => _nativeCall('media.getUserMedia', { 'constraints': constraints }); }

const clipboard = _Clipboard();
const fileSystem = _FileSystem();
const shell = _Shell();
const app = _App();
const localStorage = _WebStorage('localStorage');
const sessionStorage = _WebStorage('sessionStorage');
const navigator = _WebNavigator();
const location = _WebLocation();
const capabilities = _Capabilities();
const fylo = _Fylo();
const browser = _Browser();
const share = _Share();
const haptics = _Haptics();
const filePicker = _FilePicker();
const secrets = _Secrets();
const auth = _Auth();
const geolocation = _Geolocation();
const notifications = _Notifications();
const media = _Media();
Future<dynamic> fetch(dynamic input, [dynamic init]) => _nativeCall('web.fetch', { 'input': input.toString(), 'init': init });

${source}

dynamic _read(${contract.className} instance, String field) {
  switch (field) {
    ${readCases}
    default: return null;
  }
}

void _write(${contract.className} instance, String field, dynamic value) {
  switch (field) {
    ${writeCases}
    default: return;
  }
}

dynamic _call(${contract.className} instance, String method, dynamic rawArguments) {
  final arguments = rawArguments is JsArray ? rawArguments.toList() : const <dynamic>[];
  dynamic result;
  switch (method) {
    ${callCases}
    default: return null;
  }

  return result is Future ? _futureToPromise(result) : result;
}

JsObject _futureToPromise(Future<dynamic> future) {
  final promise = context['Promise'] as JsFunction;
  return JsObject(promise, [allowInterop((dynamic resolve, dynamic reject) {
    future.then(
      (value) => (resolve as JsFunction).apply([value]),
      onError: (Object error, StackTrace stack) => (reject as JsFunction).apply([error.toString()]),
    );
  })]);
}

void main() {
  context[${JSON.stringify(factoryName)}] = allowInterop((dynamic props, dynamic runtime) {
    __tcRuntime = runtime;
    final instance = ${contract.className}();
    instance.props = props;
    instance.tac = runtime;
    return JsObject.jsify({
      'get': allowInterop((String field) => _read(instance, field)),
      'set': allowInterop((String field, dynamic value) { _write(instance, field, value); return _read(instance, field); }),
      'call': allowInterop((String method, dynamic arguments) => _call(instance, method, arguments)),
    });
  });
}
`;
    }

    /** @param {DartCompanionContract} contract @param {string} factoryName @param {string} [runtimeUrl] */
    createJavaScriptAdapter(contract, factoryName, runtimeUrl = '') {
        const fields = JSON.stringify(contract.fields.map(({ name }) => name));
        const initialFields = JSON.stringify(Object.fromEntries(contract.fields
            .filter((field) => Object.hasOwn(field, 'initial'))
            .map((field) => [field.name, field.initial])));
        const methods = JSON.stringify(contract.methods.map(({ name }) => name));
        const subscriptions = JSON.stringify(contract.subscriptions);
        const mountMethods = JSON.stringify(contract.mountMethods);
        const publishedFields = JSON.stringify(contract.publishedFields.map((field) => ({ ...field, options: { retain: true } })));
        const assetLoader = runtimeUrl
            ? `const __tc_dart_prerender__ = Boolean(globalThis.__tc_prerender__);
if (!__tc_dart_prerender__) {
    const __tc_dart_runtime_url__ = new URL(${JSON.stringify(runtimeUrl)}, import.meta.url).href;
    await import(__tc_dart_runtime_url__);
}
`
            : '';
        return `
${assetLoader}
const __tc_dart_factory__ = globalThis[${JSON.stringify(factoryName)}];
if (!globalThis.__tc_prerender__ && typeof __tc_dart_factory__ !== 'function') {
    throw new Error('Dart Tac companion did not register ${factoryName}');
}
const __tc_dart_fields__ = ${fields};
const __tc_dart_initial_fields__ = ${initialFields};
const __tc_dart_methods__ = ${methods};
const __tc_dart_subscriptions__ = ${subscriptions};
const __tc_dart_mount_methods__ = ${mountMethods};
const __tc_dart_published_fields__ = ${publishedFields};

export default class {
    constructor(props = {}, tac = {}) {
        const bridge = typeof __tc_dart_factory__ === 'function' ? __tc_dart_factory__(props, tac) : null;
        const initial = { ...__tc_dart_initial_fields__ };
        const controller = {};
        for (const field of __tc_dart_fields__) {
            Object.defineProperty(controller, field, {
                configurable: true,
                enumerable: true,
                get: () => bridge ? bridge.get(field) : initial[field],
                set: (value) => {
                    if (bridge) bridge.set(field, value);
                    else initial[field] = value;
                },
            });
        }
        // Field getters delegate live to the Dart instance, so a completed
        // method has already mutated state by the time it returns — reactive
        // setters can never observe a change. Publish and re-render
        // explicitly instead of relying on assignment-based reactivity.
        const lastPublished = {};
        const syncAfterCall = () => {
            if (!bridge) return;
            for (const field of __tc_dart_published_fields__) {
                const value = bridge.get(field.field);
                if (!Object.is(lastPublished[field.field], value)) {
                    lastPublished[field.field] = value;
                    if (typeof tac.publish === 'function') tac.publish(field.name, value, field.options);
                }
            }
            if (typeof tac.rerender === 'function') tac.rerender();
        };
        for (const method of __tc_dart_methods__) {
            controller[method] = async (...args) => {
                if (!bridge) return undefined;
                const value = await bridge.call(method, args);
                syncAfterCall();
                return value;
            };
        }
        controller.__tc_signal_publish_fields__ = __tc_dart_published_fields__;
        for (const subscription of __tc_dart_subscriptions__) {
            tac.subscribe(subscription.name, async (value) => {
                await controller[subscription.method](value);
            }, { immediate: false });
        }
        for (const method of __tc_dart_mount_methods__) {
            tac.onMount(() => controller[method]());
        }
        return controller;
    }
}
`;
    }
}
