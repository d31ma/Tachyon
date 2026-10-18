use crate::Failure;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use topic_streams::TopicStreams;

mod topic_streams;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WebLanguage {
    Rust,
    CSharp,
    Go,
    Kotlin,
    Swift,
    Cpp,
    Java,
    Ruby,
    Lua,
    Python,
}

impl WebLanguage {
    pub(crate) const PREFERENCE: [Self; 10] = [
        Self::Rust,
        Self::Go,
        Self::CSharp,
        Self::Swift,
        Self::Cpp,
        Self::Kotlin,
        Self::Java,
        Self::Ruby,
        Self::Lua,
        Self::Python,
    ];

    const fn shared_module(self) -> &'static str {
        match self {
            Self::Rust => "tachyon-rust.js",
            Self::CSharp => "tachyon-csharp.js",
            Self::Go => "tachyon-go.js",
            Self::Kotlin => "tachyon-kotlin.js",
            Self::Swift => "tachyon-swift.js",
            Self::Cpp => "tachyon-cpp.js",
            Self::Java => "tachyon-java.js",
            Self::Ruby => "tachyon-ruby-services.js",
            Self::Lua => "tachyon-lua-services.js",
            Self::Python => "tachyon-python-services.js",
        }
    }

    const fn model_extension(self) -> &'static str {
        match self {
            Self::Rust => "rs",
            Self::CSharp => "cs",
            Self::Go => "go",
            Self::Kotlin => "kt",
            Self::Swift => "swift",
            Self::Cpp => crate::tac_cpp::CppSource::EXTENSION,
            Self::Java => "java",
            Self::Ruby => "rb",
            Self::Lua => "lua",
            Self::Python => "py",
        }
    }
}

#[derive(Default)]
pub(crate) struct BuiltServices {
    pub(crate) files: Vec<(String, Vec<u8>)>,
    pub(crate) timings: Vec<(String, Duration)>,
    pub(crate) notes: Vec<String>,
}

pub(crate) struct BuildJobs {
    permits: tokio::sync::Semaphore,
}

impl BuildJobs {
    const MOST_JOBS: usize = 4;

    pub(crate) fn from_environment() -> Self {
        let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        let configured = std::env::var("TAC_BUILD_JOBS").ok();
        Self::new(Self::limit(configured.as_deref(), cores))
    }

    pub(crate) fn new(limit: usize) -> Self {
        Self {
            permits: tokio::sync::Semaphore::new(limit.max(1)),
        }
    }

    fn limit(configured: Option<&str>, cores: usize) -> usize {
        match configured.and_then(|value| value.trim().parse::<usize>().ok()) {
            Some(jobs) if jobs > 0 => jobs,
            _ => (cores / 3).clamp(1, Self::MOST_JOBS),
        }
    }

    pub(crate) async fn run<T>(&self, work: impl Future<Output = T>) -> T {
        let _permit = self.permits.acquire().await;
        work.await
    }
}

pub(crate) async fn build_all(
    snapshot_root: &Path,
    services: &[(String, String, WebLanguage, PathBuf)],
    jobs: &BuildJobs,
    profile: crate::BuildProfile,
) -> Result<BuiltServices, Failure> {
    let mut builds = Vec::new();
    for language in WebLanguage::PREFERENCE {
        let chosen = services_in_language(services, language);
        if !chosen.is_empty() {
            builds.push(jobs.run(build_language(snapshot_root, language, chosen, profile)));
        }
    }
    let mut built = BuiltServices::default();
    for result in futures_util::future::join_all(builds).await {
        let (language, files, took, note) = result?;
        built.files.extend(files);
        built.timings.push((format!("{language:?}"), took));
        built.notes.extend(note);
    }
    Ok(built)
}

async fn build_language(
    snapshot_root: &Path,
    language: WebLanguage,
    chosen: Vec<(String, String, PathBuf)>,
    profile: crate::BuildProfile,
) -> Result<
    (
        WebLanguage,
        Vec<(String, Vec<u8>)>,
        Duration,
        Option<String>,
    ),
    Failure,
> {
    let started = Instant::now();
    let mut published = Vec::new();
    let mut note = None;
    let engine = match language {
        WebLanguage::Rust => {
            let module = crate::native::web_assembly(snapshot_root, &chosen, profile).await?;
            note = module.note;
            WebEngine::linear_memory(&WebEngine::RUST_INSTANCE.replace(
                "__WASM__",
                &crate::compiler::ContentDigest::hex(&module.binary),
            ))
        }
        WebLanguage::CSharp => {
            let (framework, reason) = dotnet_framework(snapshot_root, &chosen, profile).await?;
            published.extend(framework);
            note = reason;
            String::from(WebEngine::CSHARP_ENGINE)
        }
        WebLanguage::Go => {
            let built = crate::native::web_go(snapshot_root, &chosen, profile).await?;
            note = built.note;
            published.push((String::from(WebEngine::GO_MODULE), built.wasm));
            published.push((
                String::from(WebEngine::GO_GLUE),
                WebEngine::shipped_glue(built.glue),
            ));
            String::from(WebEngine::GO_ENGINE)
        }
        WebLanguage::Kotlin => {
            let (program, reason) =
                crate::native::KotlinWebServices::build(snapshot_root, &chosen, profile).await?;
            published.extend(program);
            note = reason;
            String::from(WebEngine::KOTLIN_ENGINE)
        }
        WebLanguage::Swift => {
            let (program, reason) =
                crate::native::SwiftWebServices::build(snapshot_root, &chosen, profile).await?;
            published.extend(program);
            note = reason;
            WebEngine::linear_memory(WebEngine::SWIFT_INSTANCE)
        }
        WebLanguage::Cpp => {
            let (program, reason) =
                crate::native::CppWebServices::build(snapshot_root, &chosen, profile).await?;
            published.extend(program);
            note = reason;
            WebEngine::linear_memory(crate::native::CppWebServices::INSTANCE)
        }
        WebLanguage::Java => {
            let (program, reason) =
                crate::native::JavaWebServices::build(snapshot_root, &chosen, profile).await?;
            published.extend(program);
            note = reason;
            String::from(WebEngine::JAVA_ENGINE)
        }
        WebLanguage::Python => {
            let built = crate::tac_python::PythonWebServices::build(snapshot_root, &chosen)?;
            published.extend(built.assets);
            built.engine
        }
        WebLanguage::Ruby => {
            let engine = ruby_engine(snapshot_root, &chosen)?;
            published.extend(crate::tac_ruby::RubyWebServices::runtime_files().await?);
            engine
        }
        WebLanguage::Lua => {
            let engine = crate::tac_lua::LuaWebServices::engine(snapshot_root, &chosen)?;
            published.extend(crate::tac_lua::LuaWebServices::runtime_files().await?);
            engine
        }
    };
    let shared = language.shared_module();
    published.push((
        shared.to_owned(),
        WebEngine::shared_source(&engine).into_bytes(),
    ));
    let topics = TopicStreams::read(snapshot_root, language.model_extension())?;
    for (instance, class, _) in &chosen {
        let service_module = module(instance, class, shared, &topics);
        published.push((format!("{class}.js"), service_module.into_bytes()));
    }
    Ok((language, published, started.elapsed(), note))
}

fn services_in_language(
    services: &[(String, String, WebLanguage, PathBuf)],
    language: WebLanguage,
) -> Vec<(String, String, PathBuf)> {
    let mut chosen = Vec::new();
    for (instance, class, used, source) in services {
        if *used == language {
            chosen.push((instance.clone(), class.clone(), source.clone()));
        }
    }
    chosen
}

async fn dotnet_framework(
    snapshot_root: &Path,
    chosen: &[(String, String, PathBuf)],
    profile: crate::BuildProfile,
) -> Result<(Vec<(String, Vec<u8>)>, Option<String>), Failure> {
    let (framework, note) = crate::native::web_dotnet(snapshot_root, chosen, profile).await?;
    let mut files = Vec::new();
    for (name, bytes) in framework {
        files.push((format!("dotnet/{name}"), bytes));
    }
    Ok((files, note))
}

fn ruby_engine(
    snapshot_root: &Path,
    chosen: &[(String, String, PathBuf)],
) -> Result<String, Failure> {
    let program = crate::tac_ruby::RubyWebServices::program(snapshot_root, chosen)?;
    let literal = serde_json::to_string(&program).unwrap_or_default();
    Ok(WebEngine::RUBY_ENGINE.replace("__RUBY_PROGRAM__", &literal))
}

fn module(instance: &str, class: &str, shared: &str, topics: &serde_json::Value) -> String {
    let plan = serde_json::json!({ "types": [{
        "name": class,
        "post_construct": "__tachyonStarted",
        "pre_destroy": "__tachyonDispose",
        "resources": [{ "field": "__tachyonPublisher", "name": "applicationEventPublisher" }],
        "listeners": [],
        "topics": topics,
    }] });
    let instance = serde_json::to_string(instance).unwrap_or_default();
    format!(
        "// Generated by Tachyon (ADR 0035, ADR 0037): {class}, compiled to WebAssembly.\n\
         import {{ activate }} from './{shared}'\n\
         export default class {class} {{\n\
         \x20 constructor(properties) {{ activate(this, {instance}, properties, __tachyon_event_plan__.types[0]) }}\n\
         }}\n\
         export const __tachyon_event_plan__ = {plan}\n"
    )
}

pub(crate) fn base64(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut triple = 0u32;
        for (index, byte) in chunk.iter().enumerate() {
            triple |= u32::from(*byte) << (16 - 8 * index);
        }
        for index in 0..4 {
            if index > chunk.len() {
                encoded.push('=');
                continue;
            }
            let sextet = (triple >> (18 - 6 * index)) & 0x3f;
            encoded.push(char::from(WebEngine::BASE64_ALPHABET[sextet as usize]));
        }
    }
    encoded
}

struct WebEngine;

impl WebEngine {
    fn shared_source(engine: &str) -> String {
        let head = Self::SHARED_HEAD;
        let tail = Self::SHARED_TAIL;
        crate::template::ShippedScript::compact_keeping_notice(&format!("{head}{engine}{tail}"))
    }

    fn shipped_glue(glue: Vec<u8>) -> Vec<u8> {
        String::from_utf8(glue).map_or_else(std::string::FromUtf8Error::into_bytes, |text| {
            Self::compacted_glue(text).into_bytes()
        })
    }

    fn compacted_glue(text: String) -> String {
        if crate::template::ShippedScript::compacts_safely(&text) {
            crate::template::ShippedScript::compact_keeping_notice(&text)
        } else {
            text
        }
    }

    fn linear_memory(instance: &str) -> String {
        format!(
            "{}{instance}{}",
            Self::LINEAR_MEMORY_HOST,
            Self::LINEAR_MEMORY_TRANSPORT
        )
    }

    const BASE64_ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    const SHARED_HEAD: &str = r"// Generated by Tachyon (ADR 0035, ADR 0037): the page's side of the companion
// protocol, around every web service of one language.
const published = []
const storage = globalThis[Symbol.for('tachyon.tac.storage')]
const storageHost = storage ? (text) => storage.host(text) : undefined
const web = globalThis[Symbol.for('tachyon.tac.web')]
const webHost = (text) => web ? web.host(text) : JSON.stringify({ error: 'Tachyon browser APIs are not on this page.' })
const calling = (owner, work) => web ? web.calling(owner, work) : work()
await storage?.mirror()
";

    const LINEAR_MEMORY_HOST: &str = r"const encoder = new TextEncoder()
const decoder = new TextDecoder()
const wasmRead = (pointer, length) => decoder.decode(new Uint8Array(exports.memory.buffer, pointer, length))
const hostAnswer = (host, pointer, length) => {
  const answer = encoder.encode(`${host(decoder.decode(new Uint8Array(exports.memory.buffer, pointer, length)))}\0`)
  const target = exports.tac_wasm_alloc(answer.length)
  new Uint8Array(exports.memory.buffer, target, answer.length).set(answer)
  return target
}
const tachyonImports = {
  tac_host_emit: (pointer, length) => { published.push(decoder.decode(new Uint8Array(exports.memory.buffer, pointer, length))) },
  tac_host_monotonic_ms: () => performance.now(),
  tac_host_epoch_ms: () => Date.now(),
  tac_host_storage: (pointer, length) => storageHost ? hostAnswer(storageHost, pointer, length) : 0,
  tac_host_web: (pointer, length) => hostAnswer(webHost, pointer, length),
}
";

    const RUST_INSTANCE: &str = concat!(
        include_str!("native/browser/tac-task-answers.js"),
        include_str!("native/browser/tac-wasm-tasks.js"),
        r"const wasmTasks = new TacWasmTasks()
const wasmText = '__WASM__'
const binary = new Uint8Array(wasmText.length / 2)
for (let index = 0; index < binary.length; index += 1) binary[index] = parseInt(wasmText.slice(index * 2, index * 2 + 2), 16)
const { instance } = await WebAssembly.instantiate(binary, { tachyon: { ...tachyonImports, ...wasmTasks.imports(wasmRead) } })
const exports = instance.exports
wasmTasks.attach(exports)
"
    );

    const SWIFT_INSTANCE: &str = concat!(
        include_str!("native/browser/tac-task-answers.js"),
        include_str!("native/browser/tac-wasm-tasks.js"),
        r"const wasmTasks = new TacWasmTasks()
const swiftResponse = await fetch(new URL('./swift/services.wasm', import.meta.url))
const swiftModule = await WebAssembly.compile(await swiftResponse.arrayBuffer())
const wasiNotSupported = 52
const wasiBadDescriptor = 8
const wasiView = () => new DataView(exports.memory.buffer)
const wasiPending = { 1: '', 2: '' }
const wasiWrite = (descriptor, text) => {
  const lines = (wasiPending[descriptor] + text).split('\n')
  wasiPending[descriptor] = lines.pop()
  for (const line of lines) (descriptor === 2 ? console.error : console.log)(line)
}
const wasiNothing = (count, size) => { wasiView().setUint32(count, 0, true); wasiView().setUint32(size, 0, true); return 0 }
const wasiNow = (clock) => clock === 0 ? Date.now() : performance.now()
const wasiClockWait = (view, subscription) => {
  const milliseconds = Number(view.getBigUint64(subscription + 24, true)) / 1e6
  const absolute = (view.getUint16(subscription + 40, true) & 1) === 1
  return absolute ? milliseconds - wasiNow(view.getUint32(subscription + 16, true)) : milliseconds
}
const wasi = {
  args_get: () => 0,
  args_sizes_get: wasiNothing,
  environ_get: () => 0,
  environ_sizes_get: wasiNothing,
  clock_time_get: (clock, precision, target) => {
    wasiView().setBigUint64(target, BigInt(Math.round(wasiNow(clock) * 1e6)), true)
    return 0
  },
  poll_oneoff: (subscriptions, events, count, happened) => {
    const view = wasiView()
    let soonest = -1
    let wait = Infinity
    for (let index = 0; index < count; index += 1) {
      const subscription = subscriptions + index * 48
      if (view.getUint8(subscription + 8) !== 0) return wasiNotSupported
      const milliseconds = wasiClockWait(view, subscription)
      if (milliseconds < wait) [soonest, wait] = [subscription, milliseconds]
    }
    if (soonest < 0) return wasiNotSupported
    const end = performance.now() + wait
    while (performance.now() < end);
    new Uint8Array(exports.memory.buffer, events, 32).fill(0)
    view.setBigUint64(events, view.getBigUint64(soonest, true), true)
    view.setUint32(happened, 1, true)
    return 0
  },
  random_get: (pointer, length) => {
    for (let offset = 0; offset < length; offset += 65536) {
      crypto.getRandomValues(new Uint8Array(exports.memory.buffer, pointer + offset, Math.min(65536, length - offset)))
    }
    return 0
  },
  fd_write: (descriptor, vectors, count, written) => {
    const view = wasiView()
    let total = 0
    for (let index = 0; index < count; index += 1) {
      const pointer = view.getUint32(vectors + index * 8, true)
      const length = view.getUint32(vectors + index * 8 + 4, true)
      if (descriptor === 1 || descriptor === 2) wasiWrite(descriptor, decoder.decode(new Uint8Array(exports.memory.buffer, pointer, length)))
      total += length
    }
    view.setUint32(written, total, true)
    return 0
  },
  fd_prestat_get: () => wasiBadDescriptor,
  proc_exit: (code) => { throw new Error(`Swift services stopped with exit code ${code}.`) },
}
for (const { module, name, kind } of WebAssembly.Module.imports(swiftModule)) {
  if (module === 'wasi_snapshot_preview1' && kind === 'function') wasi[name] ??= () => wasiNotSupported
}
const instance = await WebAssembly.instantiate(swiftModule, { tachyon: { ...tachyonImports, ...wasmTasks.imports(wasmRead) }, wasi_snapshot_preview1: wasi })
const exports = instance.exports
exports._initialize()
wasmTasks.attach(exports)
"
    );

    const LINEAR_MEMORY_TRANSPORT: &str = r"const transport = (text) => {
  const bytes = encoder.encode(`${text}\0`)
  const pointer = exports.tac_wasm_alloc(bytes.length)
  try {
    new Uint8Array(exports.memory.buffer, pointer, bytes.length).set(bytes)
    const result = exports.tac_native_invoke(pointer)
    const memory = new Uint8Array(exports.memory.buffer)
    const answer = decoder.decode(memory.subarray(result, memory.indexOf(0, result)))
    exports.tac_native_free(result)
    return answer
  } finally {
    exports.tac_wasm_dealloc(pointer, bytes.length)
  }
}
/** @param {string} text @returns {Promise<string>} */
const transportAsync = (text) => wasmTasks.answers.answer(transport(text))
";

    const CSHARP_ENGINE: &str = concat!(
        include_str!("native/browser/tac-dotnet-http.js"),
        r"
const { dotnet } = await import('./dotnet/_framework/dotnet.js')
const runtime = await dotnet.create()
const http = new TacDotnetHttp()
runtime.setModuleImports('tachyon', {
  emit: (payload) => { published.push(payload) },
  storage: (text) => storageHost ? storageHost(text) : JSON.stringify({ error: 'Tachyon storage is not on this page.' }),
  web: (text) => webHost(text),
  httpStart: (text) => http.start(text),
  httpAnswer: (ticket) => http.answer(ticket),
  httpCancel: (ticket) => http.cancel(ticket),
  socketSend: (owner, text) => socketHost(owner, text),
})
const companion = (await runtime.getAssemblyExports(runtime.getConfig().mainAssemblyName)).TacWeb
const transport = (text) => companion.Invoke(text)
/** @param {string} text @returns {Promise<string>} */
const transportAsync = (text) => companion.InvokeAsync(text)
"
    );

    const GO_MODULE: &str = "go/services.wasm";
    const GO_GLUE: &str = "go/wasm_exec.js";

    const GO_ENGINE: &str = concat!(
        include_str!("native/browser/tac-task-answers.js"),
        r"const goFileSystem = () => {
  const notImplemented = () => Object.assign(new Error('not implemented'), { code: 'ENOSYS' })
  const decoder = new TextDecoder()
  const output = { pending: '' }
  const fileSystem = {
    constants: { O_WRONLY: -1, O_RDWR: -1, O_CREAT: -1, O_TRUNC: -1, O_APPEND: -1, O_EXCL: -1, O_DIRECTORY: -1 },
    writeSync(descriptor, bytes) {
      output.pending += decoder.decode(bytes)
      const lineEnd = output.pending.lastIndexOf('\n')
      if (lineEnd !== -1) {
        console.log(output.pending.substring(0, lineEnd))
        output.pending = output.pending.substring(lineEnd + 1)
      }
      return bytes.length
    },
    write(descriptor, bytes, offset, length, position, callback) {
      if (offset !== 0 || length !== bytes.length || position !== null) {
        callback(notImplemented())
        return
      }
      callback(null, fileSystem.writeSync(descriptor, bytes))
    },
    fsync(descriptor, callback) { callback(null) },
  }
  for (const name of ['chmod', 'chown', 'close', 'fchmod', 'fchown', 'fstat', 'ftruncate', 'lchown', 'link', 'lstat', 'mkdir', 'open', 'read', 'readdir', 'readlink', 'rename', 'rmdir', 'stat', 'symlink', 'truncate', 'unlink', 'utimes']) {
    fileSystem[name] = (...values) => values.at(-1)(notImplemented())
  }
  return fileSystem
}
await import('./go/wasm_exec.js')
const go = new Go()
const response = await fetch(new URL('./go/services.wasm', import.meta.url))
const { instance } = await WebAssembly.instantiate(await response.arrayBuffer(), go.importObject)
const pageFileSystem = globalThis.fs
const pageOwnsFileSystem = typeof pageFileSystem?.writeSync !== 'function'
if (pageOwnsFileSystem) globalThis.fs = goFileSystem()
try {
  go.run(instance)
} finally {
  if (pageOwnsFileSystem) globalThis.fs = pageFileSystem
}
const taskAnswers = new TacTaskAnswers()
const invokeGo = globalThis.__tachyonGo((payload) => { published.push(payload) }, storageHost, webHost, (task, text) => { taskAnswers.done(task, text) }, (owner, text) => socketHost(owner, text))
delete globalThis.__tachyonGo
const transport = (text) => invokeGo(text)
"
    );

    const KOTLIN_ENGINE: &str = concat!(
        include_str!("native/browser/tac-task-answers.js"),
        include_str!("native/browser/tac-ticket-http.js"),
        r"const garbageCollected = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 95, 1, 120, 0])
if (typeof WebAssembly !== 'object' || !WebAssembly.validate(garbageCollected)) {
  throw new Error('Kotlin services need a browser with WebAssembly garbage collection: Chrome or Edge 119, Firefox 120 or Safari 18.2 and newer.')
}
const taskAnswers = new TacTaskAnswers()
const kotlinHttp = new TacTicketHttp()
globalThis[Symbol.for('tachyon.kotlin.host')] = {
  emit: (payload) => { published.push(payload) },
  storage: (text) => storageHost ? storageHost(text) : JSON.stringify({ error: 'Tachyon storage is not on this page.' }),
  web: (text) => webHost(text),
  httpStart: (ticket, text) => { kotlinHttp.start(ticket, text, (answer) => kotlin.tacHttpSettle(ticket, answer)) },
  httpCancel: (ticket) => { kotlinHttp.cancel(ticket) },
  taskDone: (task, text) => { taskAnswers.done(task, text) },
}
const kotlin = await import('./kotlin/services.js')
const transport = (text) => kotlin.tacInvoke(text)
/** @param {string} text @returns {Promise<string>} */
const transportAsync = (text) => taskAnswers.answer(transport(text))
"
    );

    const JAVA_ENGINE: &str = concat!(
        include_str!("native/browser/tac-task-answers.js"),
        include_str!("native/browser/tac-ticket-http.js"),
        r"const garbageCollected = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 95, 1, 120, 0])
if (typeof WebAssembly !== 'object' || !WebAssembly.validate(garbageCollected)) {
  throw new Error('Java services need a browser with WebAssembly garbage collection: Chrome or Edge 119, Firefox 120 or Safari 18.2 and newer.')
}
const taskAnswers = new TacTaskAnswers()
const javaHttp = new TacTicketHttp()
globalThis[Symbol.for('tachyon.java.host')] = {
  emit: (payload) => { published.push(payload) },
  storage: (text) => storageHost ? storageHost(text) : JSON.stringify({ error: 'Tachyon storage is not on this page.' }),
  web: (text) => webHost(text),
  pause: (milliseconds) => {
    const end = performance.now() + milliseconds
    while (performance.now() < end);
  },
  httpStart: (ticket, text) => { javaHttp.start(ticket, text, (answer) => { java.exports.tacHttpSettle(ticket, answer) }) },
  httpCancel: (ticket) => { javaHttp.cancel(ticket) },
  taskDone: (task, text) => { taskAnswers.done(task, text) },
}
const { load } = await import('./java/runtime.js')
const javaResponse = await fetch(new URL('./java/services.wasm', import.meta.url))
const java = await load(new Uint8Array(await javaResponse.arrayBuffer()))
const transport = (text) => java.exports.tacInvoke(text)
"
    );

    const RUBY_ENGINE: &str = concat!(
        include_str!("native/browser/tac-task-answers.js"),
        include_str!("native/browser/tac-ticket-http.js"),
        r"const { TachyonRuby } = await import('./ruby/tachyon-ruby.js')
const rubyResponse = await fetch(new URL('./ruby/tachyon-ruby.wasm', import.meta.url))
const rubyModule = await WebAssembly.compile(await rubyResponse.arrayBuffer())
const writeLine = (line, descriptor) => { if (descriptor === 2) console.error(line); else console.log(line) }
const taskAnswers = new TacTaskAnswers()
const rubyHttp = new TacTicketHttp()
const rubySettle = (ticket, answer) => {
  try {
    ruby.invoke(`tachyon-http:settle ${ticket}\n${answer}`)
  } finally {
    deliver()
  }
}
const rubyTasks = (request) => {
  if (request.http === 'start') rubyHttp.start(request.ticket, request.request, (answer) => rubySettle(request.ticket, answer))
  if (request.http === 'cancel') rubyHttp.cancel(request.ticket)
  if (request.http === 'done') taskAnswers.done(request.task, request.answer)
  return JSON.stringify({ value: null })
}
const rubyHost = (text) => {
  const request = JSON.parse(text)
  if (request.api) return webHost(text)
  if (request.http) return rubyTasks(request)
  return storageHost ? storageHost(text) : JSON.stringify({ error: 'Tachyon storage is not on this page.' })
}
const ruby = await TachyonRuby.start(rubyModule, writeLine, (payload) => { published.push(payload) }, rubyHost)
const bootFailure = ruby.boot(__RUBY_PROGRAM__)
if (bootFailure) throw new Error(`Ruby services did not start: ${JSON.parse(bootFailure).error}`)
const transport = (text) => ruby.invoke(text)
"
    );

    const SHARED_TAIL: &str = r"const publishers = new Map()
const shells = new Map()

const socketHost = async (owner, text) => {
  try {
    const { client, message } = JSON.parse(text)
    const send = shells.get(owner)?.deref()?.[`__tachyonSend:${client}`]
    if (typeof send !== 'function') return JSON.stringify({ error: { code: 'HTTP_MISSING' } })
    await send(message)
    return JSON.stringify({ value: null })
  } catch {
    return JSON.stringify({ error: { code: 'HTTP_NETWORK' } })
  }
}

const deliver = () => {
  for (const payload of published.splice(0)) {
    const signal = JSON.parse(payload)
    publishers.get(signal.owner)?.(signal.name, signal.value, signal.retain === true)
  }
}

const invoke = (request) => {
  let answer
  try {
    answer = JSON.parse(transport(JSON.stringify(request)))
  } finally {
    deliver()
  }
  if (Number.isSafeInteger(answer?.task) && typeof taskAnswers === 'object') return laterAnswer(answer.task, request)
  if (!answer || answer.error) throw new Error(`Web service ${request.service}: ${answer?.error ?? 'no answer'}`)
  return answer.value
}

/** @param {{value?: unknown, error?: string, code?: string} | undefined} answer @param {Record<string, unknown>} request @returns {unknown} */
const settledAnswer = (answer, request) => {
  if (!answer || answer.error) throw Object.assign(new Error(`Web service ${request.service}: ${answer?.error ?? 'no answer'}`), { code: answer?.code })
  return answer.value
}

/** @param {number} task @param {Record<string, unknown>} request @returns {Promise<unknown>} */
const laterAnswer = async (task, request) => {
  let answer
  try {
    answer = JSON.parse(await taskAnswers.wait(task))
  } finally {
    deliver()
  }
  return settledAnswer(answer, request)
}

/** @param {Record<string, unknown>} request @returns {Promise<unknown>} */
const invokeAsync = async (request) => {
  /** @type {{value?: unknown, error?: string, code?: string} | undefined} */
  let answer
  try {
    if (typeof transportAsync !== 'function') throw new Error('This web service has no asynchronous transport.')
    answer = JSON.parse(await transportAsync(JSON.stringify(request)))
  } finally {
    deliver()
  }
  return settledAnswer(answer, request)
}

/** @param {() => boolean} current @returns {void} */
const requireTaskTarget = (current) => {
  if (!current()) throw Object.assign(new Error('Web service task owner was disposed.'), { code: 'CANCELLED' })
}
/** @param {Record<string, unknown>} request @param {() => boolean} current @param {() => void} changed @returns {Promise<unknown>} */
const taskChange = async (request, current, changed) => {
  requireTaskTarget(current)
  try {
    const answer = await invokeAsync(request)
    requireTaskTarget(current)
    return answer
  } finally {
    changed()
  }
}

/** @param {() => boolean} current @returns {(work: () => unknown) => Promise<unknown>} */
const taskQueue = (current) => {
  /** @type {Promise<unknown>} */
  let tail = Promise.resolve()
  return (work) => {
    const next = tail.then(() => { requireTaskTarget(current); return work() })
    tail = next.then(() => undefined, () => undefined)
    return next
  }
}

const ownerId = () => {
  if (typeof crypto.randomUUID === 'function') return crypto.randomUUID()
  const hex = [...crypto.getRandomValues(new Uint8Array(16))].map((byte) => byte.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

const released = typeof FinalizationRegistry === 'function'
  ? new FinalizationRegistry((request) => { publishers.delete(request.owner); shells.delete(request.owner); try { invoke(request) } catch {} })
  : null
const authored = (values) => (typeof Event === 'function' && values[0] instanceof Event ? values.slice(1) : values)
const hidden = (target, name, value) => Object.defineProperty(target, name, { configurable: true, value })

const props = (properties) => {
  try { return JSON.parse(JSON.stringify(properties ?? {})) ?? {} } catch { return {} }
}

const topicStreams = (plan, listeners) => {
  const streams = new Map()
  for (const listener of listeners) {
    const [event] = listener.events
    const path = plan.topics?.[event]
    if (path && !streams.has(path)) streams.set(path, { method: '', path, event, socket: false, models: listener.models, topic: true })
  }
  return [...streams.values()]
}

const heldStreams = (streams) => streams.slice(0, 16)
  .filter((stream) => typeof stream?.path === 'string' && stream.path.startsWith('/') && !stream.path.startsWith('//'))
  .map(({ client, path, event, socket, models }) => ({ method: '__tachyonStream', path, event, socket: socket === true, models, topic: false, ...(socket === true ? { sender: `__tachyonSend:${client}` } : {}) }))

const listen = (target, call, plan, listeners, streams) => {
  if (!plan.listeners.length) {
    plan.listeners = listeners.map((listener) => ({ ...listener, method: `__tachyonListen:${listener.key}` }))
    plan.streams = [...heldStreams(streams), ...topicStreams(plan, listeners)]
  }
  hidden(target, '__tachyonStream', () => {})
  for (const listener of listeners) {
    hidden(target, `__tachyonListen:${listener.key}`, (value) => { call({ op: 'event', name: listener.key, value }) })
  }
}

export const activate = (target, service, properties, plan) => {
  const owner = ownerId()
  const early = []
  const self = new WeakRef(target)
  /** @type {boolean} */
  let disposed = false
  /** @returns {boolean} */
  const current = () => !disposed
  /** @returns {void} */
  const changed = () => { if (!disposed) self.deref()?.__tachyonStoredChanged?.() }
  const call = (request) => calling(self.deref() ?? null, () => invoke({ ...request, route: 'web', service, owner }))
  /** @param {Record<string, unknown>} request @returns {Promise<unknown>} */
  const changeAsync = (request) => taskChange({ ...request, route: 'web', service, owner }, current, changed)
  const change = (request) => {
    let result
    try {
      result = call(request)
      return result
    } finally {
      if (typeof result?.then === 'function') result.then(changed, changed)
      else changed()
    }
  }
  shells.set(owner, self)
  publishers.set(owner, (name, value, retain) => {
    const publisher = self.deref()?.__tachyonPublisher
    if (publisher && !early.length) publisher.publishEvent(name, value, retain)
    else early.push([name, value, retain])
  })
  const declared = call({ op: 'init', props: props(properties) })
  const exclusive = declared?.exclusive === true
  const queue = taskQueue(current)
  const asynchronous = new Set(declared?.asyncMethods ?? [])
  /** @type {boolean} */
  let busy = false
  /** @type {Map<string, unknown>} */
  let snapshot = new Map()
  const gated = (send) => (request) => busy ? queue(() => send(request)) : send(request)
  const changeGated = gated(change)
  const callGated = gated(call)
  const startTask = (request) => {
    snapshot = new Map((declared?.fields ?? []).map((name) => [name, call({ op: 'get', name })]))
    busy = true
    return taskChange({ ...request, route: 'web', service, owner }, current, () => { busy = false; changed() })
  }
  const run = (request) => {
    if (!asynchronous.has(request.name)) return changeGated(request)
    return exclusive ? queue(() => startTask(request)) : changeAsync(request)
  }
  listen(target, changeGated, plan, declared?.listeners ?? [], Array.isArray(declared?.streams) ? declared.streams : [])
  const dispose = { op: 'dispose', route: 'web', service, owner }
  released?.register(target, dispose, target)
  hidden(target, '__tachyonStarted', () => {
    for (const [name, value, retain] of early.splice(0)) target.__tachyonPublisher.publishEvent(name, value, retain)
  })
  hidden(target, '__tachyonDispose', () => {
    disposed = true
    released?.unregister(target)
    publishers.delete(owner)
    shells.delete(owner)
    invoke(dispose)
  })
  for (const name of declared?.fields ?? []) {
    Object.defineProperty(target, name, {
      configurable: true,
      enumerable: true,
      get: () => busy ? snapshot.get(name) : call({ op: 'get', name }),
      set: (value) => { callGated({ op: 'set', name, value }) },
    })
  }
  for (const name of declared?.methods ?? []) {
    Object.defineProperty(target, name, {
      configurable: true,
      writable: true,
      value: (...values) => run({ op: 'call', name, args: authored(values) }),
    })
  }
}
";
}

#[cfg(test)]
mod dotnet_http_browser_fixture;
#[cfg(test)]
mod dotnet_http_cache_browser_fixture;
#[cfg(test)]
mod dotnet_task_browser_fixture;
#[cfg(test)]
mod dotnet_task_collection_browser_fixture;
#[cfg(test)]
mod dotnet_task_model_browser_fixture;
#[cfg(test)]
mod dotnet_task_tests;
#[cfg(test)]
mod first_app_csharp_browser_fixture;
#[cfg(test)]
mod fsharp_http_browser_fixture;
#[cfg(test)]
mod ordinary_browser_fixture;
#[cfg(test)]
mod reach_tests;

#[cfg(test)]
mod go_stream_tests;

#[cfg(test)]
mod csharp_stream_tests;

#[cfg(test)]
mod rust_embedding_tests;

#[cfg(test)]
mod shipped_glue_tests;

#[cfg(test)]
mod web_api_tests;

#[cfg(test)]
mod preference_strings_tests;

#[cfg(test)]
mod preference_strings_browser_fixture;

#[cfg(test)]
mod document_title_browser_fixture;

#[cfg(test)]
mod kotlin_tests;

#[cfg(test)]
pub(crate) mod swift_tests;

#[cfg(test)]
mod java_behavior_tests;
#[cfg(test)]
mod java_tests;

#[cfg(test)]
mod fsharp_tests;

#[cfg(test)]
mod model_binding_tests;

#[cfg(test)]
mod converter_model_tests;

#[cfg(test)]
mod go_needs_tests;

#[cfg(test)]
mod go_pagehide_tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use std::process::Command;

    struct Fixtures;

    impl Fixtures {
        const COUNTER_MODELS: [(&str, &str); 2] = [
            (
                "client/models/CounterStarted.rs",
                "pub struct CounterStarted {}\n",
            ),
            (
                "client/models/CounterAdded.rs",
                "pub struct CounterAdded {\n    pub count: i64,\n}\n",
            ),
        ];

        const COUNTER_SERVICE: &str = "use crate::models::{CounterAdded, CounterStarted};\n\n#[Service()]\n#[Lifetime({\"value\":\"prototype\"})]\n#[NeedsAll]\n#[derive(Default)]\nstruct CounterService {\n    count: i64,\n    label: String,\n    application_event_publisher: ApplicationEventPublisher,\n}\n\n\
             impl CounterService {\n    #[OnStart()]\n    fn start(&mut self) {\n        self.label.push_str(\" clicks\");\n        \
             self.application_event_publisher.publish_event(CounterStarted {}, false);\n    }\n\n    \
             fn increment(&mut self) -> i64 {\n        self.count += 1;\n        \
             self.application_event_publisher.publish_event(CounterAdded { count: self.count }, false);\n        self.count\n    }\n\n    \
             fn doubled(&self) -> i64 {\n        self.count * 2\n    }\n\n    \
             fn add(&mut self, by: i64, why: &str) -> String {\n        self.count += by;\n        format!(\"{why} {}\", self.count)\n    }\n}\n";

        const COUNTER_DRIVER: &str = "import CounterService, { __tachyon_event_plan__ as plan } from './CounterService.js'\n\
             const events = []\n\
             const one = new CounterService({ label: 'many', ignored: 1 }), two = new CounterService({})\n\
             for (const [tag, owner] of [['one', one], ['two', two]]) {\n\
             \x20 Object.defineProperty(owner, '__tachyonPublisher', { value: { publishEvent: (event, value) => events.push([tag, event, value]) } })\n\
             \x20 owner.__tachyonStarted()\n\
             }\n\
             one.increment(); one.increment(); two.increment()\n\
             const doubled = one.doubled()\n\
             one.count = 40\n\
             const after = one.increment()\n\
             const label = one.label\n\
             const added = one.add(new Event('click'), 9, 'now')\n\
             let bigStart = '', bigCall = '', largeStart = ''\n\
             try { new CounterService({ label: 'x'.repeat(1100000) }) } catch (error) { bigStart = error.message }\n\
             try { new CounterService({ label: 'x'.repeat(921600) }).__tachyonDispose() } catch (error) { largeStart = error.message }\n\
             try { one.add(new Event('click'), 1, 'x'.repeat(70000)) } catch (error) { bigCall = error.message }\n\
             one.__tachyonDispose()\n\
             let stale = false\n\
             try { one.count } catch { stale = true }\n\
             console.log(JSON.stringify({ doubled, after, other: two.count, label, otherLabel: two.label, added, stale, events, keys: Object.keys(two), plan: plan.types[0], bigStart, bigCall, largeStart }))\n";

        const GO_COUNTER_MODELS: [(&str, &str); 2] = [
            (
                "client/models/CounterStarted.go",
                "type CounterStarted struct{}\n",
            ),
            (
                "client/models/CounterAdded.go",
                "type CounterAdded struct {\n\tCount int64 `json:\"count\"`\n}\n",
            ),
        ];

        const GO_STORAGE_FILES: [(&str, &str); 3] = [
            (
                "client/setups/BrowserSetup.go",
                "// @Setup\ntype BrowserSetup struct{}\n\n\
                 func (BrowserSetup) Configure(execution *BehaviorExecution) (*BehaviorConfiguration, error) {\n\
                 \treturn NewBehaviorConfiguration(), nil\n}\n",
            ),
            (
                "client/entities/NoteEntity.go",
                "// @Entity({\"adapter\":\"indexeddb\"})\n// @Table(\"notes\")\n\
                 type NoteEntity struct {\n\t// @Id\n\tid string\n\ttext string\n}\n",
            ),
            (
                "client/services/NotesService.go",
                "// @Service\n// @CacheSettings({\"name\":\"notes\",\"ttlMs\":60000,\"maxBytes\":4096,\"maxEntries\":8})\n\
                 type NotesService struct {\n\tbehaviors *BehaviorContext\n\tReads     int\n}\n\n\
                 // @OnStart\nfunc (notes *NotesService) Start() error {\n\
                 \tcontext, err := BehaviorsCreate[BrowserSetup](NewBehaviorOptions(\"notes\", NewBehaviorSignal()))\n\
                 \tnotes.behaviors = context\n\treturn err\n}\n\n\
                 // @Cached\nfunc (notes *NotesService) Find(key BehaviorValue) (BehaviorValue, error) {\n\
                 \tnotes.Reads++\n\treturn map[string]any{\"key\": key, \"reads\": notes.Reads}, nil\n}\n\n\
                 // @ClearCache({\"method\":\"Find\"})\nfunc (notes *NotesService) Forget(key BehaviorValue) (BehaviorValue, error) {\n\
                 \treturn nil, nil\n}\n\n\
                 func (notes *NotesService) Save(id BehaviorValue, text BehaviorValue) (BehaviorValue, error) {\n\
                 \trepository, err := BehaviorsRepository[NoteEntity](notes.behaviors)\n\
                 \tif err != nil {\n\t\treturn nil, err\n\t}\n\
                 \treturn repository.Save(map[string]any{\"id\": id, \"text\": text})\n}\n\n\
                 // @AllOrNothing({\"adapter\":\"indexeddb\"})\n\
                 func (notes *NotesService) SaveThenRefuse(id BehaviorValue) (BehaviorValue, error) {\n\
                 \tif _, err := notes.Save(id, \"draft\"); err != nil {\n\t\treturn nil, err\n\t}\n\
                 \treturn nil, &BehaviorError{Code: \"validation\", Message: \"the note was refused\"}\n}\n\n\
                 func (notes *NotesService) List() (BehaviorValue, error) {\n\
                 \trepository, err := BehaviorsRepository[NoteEntity](notes.behaviors)\n\
                 \tif err != nil {\n\t\treturn nil, err\n\t}\n\
                 \treturn repository.Find(map[string]any{})\n}\n",
            ),
        ];

        const RUST_STORAGE_FILES: [(&str, &str); 3] = [
            (
                "client/setups/BrowserSetup.rs",
                "#[Setup]\nstruct BrowserSetup {}\nimpl BrowserSetup {\n    \
                 fn configure(_execution: &BehaviorExecution) -> BehaviorResult<BehaviorConfiguration> {\n        \
                 Ok(BehaviorConfiguration::default())\n    }\n}\n",
            ),
            (
                "client/entities/NoteEntity.rs",
                "#[Entity({\"adapter\":\"indexeddb\"})]\n#[Table(\"notes\")]\nstruct NoteEntity {\n    #[Id]\n    id: String,\n    text: String,\n}\n",
            ),
            (
                "client/services/NotesService.rs",
                "#[Service]\n#[CacheSettings({\"name\":\"notes\",\"ttlMs\":60000,\"maxBytes\":4096,\"maxEntries\":8})]\n\
                 struct NotesService {\n    behaviors: BehaviorContext,\n    reads: std::cell::Cell<f64>,\n}\n\
                 impl NotesService {\n    fn new() -> Self {\n        \
                 let options = BehaviorOptions::new(\"notes\", BehaviorSignal::new());\n        \
                 Self {\n            behaviors: Behaviors::create::<BrowserSetup>(options).expect(\"behavior context\"),\n            \
                 reads: std::cell::Cell::new(0.0),\n        }\n    }\n\n    \
                 #[Cached]\n    fn find(&self, key: BehaviorValue) -> BehaviorResult<BehaviorValue> {\n        \
                 let reads = self.reads.get() + 1.0;\n        self.reads.set(reads);\n        \
                 Ok(BehaviorValue::object([(\"key\", key), (\"reads\", BehaviorValue::Number(reads))]))\n    }\n\n    \
                 #[ClearCache({\"method\":\"find\"})]\n    fn forget(&self, key: BehaviorValue) -> BehaviorResult<BehaviorValue> {\n        \
                 Ok(key)\n    }\n\n    \
                 fn keep(&self, id: BehaviorValue, text: BehaviorValue) -> BehaviorResult<BehaviorValue> {\n        \
                 self.behaviors.repository::<NoteEntity>()?.save(BehaviorValue::object([(\"id\", id), (\"text\", text)]))\n    }\n\n    \
                 #[AllOrNothing({\"adapter\":\"indexeddb\"})]\n    fn keep_then_refuse(&self, id: BehaviorValue) -> BehaviorResult<BehaviorValue> {\n        \
                 self.keep(id, BehaviorValue::from(\"draft\"))?;\n        \
                 Err(BehaviorError { code: \"validation\", message: \"the note was refused\", path: None })\n    }\n\n    \
                 fn answer(result: BehaviorResult<BehaviorValue>) -> String {\n        \
                 match result {\n            Ok(value) => value.to_json(65536).unwrap_or_default(),\n            \
                 Err(error) => format!(\"{{\\\"error\\\":\\\"{error}\\\"}}\"),\n        }\n    }\n\n    \
                 fn look(&self, key: String) -> String {\n        Self::answer(self.find(BehaviorValue::from(key)))\n    }\n\n    \
                 fn drop_key(&self, key: String) -> String {\n        Self::answer(self.forget(BehaviorValue::from(key)))\n    }\n\n    \
                 fn save(&self, id: String, text: String) -> String {\n        \
                 Self::answer(self.keep(BehaviorValue::from(id), BehaviorValue::from(text)))\n    }\n\n    \
                 fn refuse(&self, id: String) -> String {\n        Self::answer(self.keep_then_refuse(BehaviorValue::from(id)))\n    }\n\n    \
                 fn list(&self, size: f64) -> String {\n        \
                 let page = BehaviorValue::object([(\"size\", BehaviorValue::Number(size))]);\n        \
                 Self::answer(self.behaviors.repository::<NoteEntity>().and_then(|notes| notes.find(page)))\n    }\n}\n",
            ),
        ];

        const RUST_STORAGE_DRIVER: &str = "console.info = () => {}\n\
             globalThis.indexedDB = { databases: async () => [], open: () => ({}) }\n\
             ;(0, eval)(await readFile('./tac-storage.js', 'utf8'))\n\
             const { default: NotesService } = await import('./NotesService.js')\n\
             const notes = new NotesService({})\n\
             const first = JSON.parse(notes.look('a'))\n\
             const second = JSON.parse(notes.look('a'))\n\
             notes.dropKey('a')\n\
             const third = JSON.parse(notes.look('a'))\n\
             notes.save('n1', 'hello')\n\
             const refused = notes.refuse('n2')\n\
             console.log(JSON.stringify({ first, second, third, refused, listed: JSON.parse(notes.list(10)) }))\n\
             process.exit(0)\n";

        const STORAGE_DRIVER: &str = "console.info = () => {}\n\
             globalThis.indexedDB = { databases: async () => [], open: () => ({}) }\n\
             ;(0, eval)(await readFile('./tac-storage.js', 'utf8'))\n\
             const { default: NotesService } = await import('./NotesService.js')\n\
             const notes = new NotesService({})\n\
             const first = notes.find('a')\n\
             const second = notes.find('a')\n\
             notes.forget('a')\n\
             const third = notes.find('a')\n\
             notes.save('n1', 'hello')\n\
             let refused = ''\n\
             try { notes.saveThenRefuse('n2') } catch (error) { refused = error.message }\n\
             console.log(JSON.stringify({ first, second, third, refused, listed: notes.list() }))\n\
             process.exit(0)\n";

        const GO_SIGNAL_MODELS: [(&str, &str); 2] = [
            (
                "client/models/SignalSent.go",
                "type SignalSent struct {\n\tSize int `json:\"size\"`\n}\n",
            ),
            (
                "client/models/SignalEcho.go",
                "type SignalEcho struct {\n\tSize int `json:\"size\"`\n}\n",
            ),
        ];

        const GO_COUNTER_SERVICE: &str = "import \"strconv\"\n\n// @Service()\n// @Lifetime({\"value\":\"prototype\"})\n// @NeedsAll\ntype CounterService struct {\n\tCount int64\n\tLabel string\n\tpublisher ApplicationEventPublisher\n}\n\n// @OnStart()\nfunc (counter *CounterService) Start() {\n\tcounter.Label += \" clicks\"\n\tcounter.publisher.PublishEvent(CounterStarted{}, false)\n}\n\nfunc (counter *CounterService) Increment() int64 {\n\tcounter.Count++\n\tcounter.publisher.PublishEvent(CounterAdded{Count: counter.Count}, false)\n\treturn counter.Count\n}\n\nfunc (counter *CounterService) Doubled() int64 {\n\treturn counter.Count * 2\n}\n\nfunc (counter *CounterService) Add(by int64, why string) string {\n\tcounter.Count += by\n\treturn why + \" \" + strconv.FormatInt(counter.Count, 10)\n}\n";

        const GO_SIGNAL_SERVICE: &str = "// @Service()\ntype SignalService struct {\n\tLarge     []int\n\tEchoed    int\n\tpublisher ApplicationEventPublisher\n}\n\nfunc (signal *SignalService) Send(size int) {\n\tsignal.publisher.PublishEvent(SignalSent{Size: size}, false)\n}\n\n// @OnEvent({\"condition\":\"value.size > 5\"})\nfunc (signal *SignalService) Record(value SignalSent) SignalEcho {\n\tsignal.Large = append(signal.Large, value.Size)\n\treturn SignalEcho{Size: value.Size * 10}\n}\n\n// @OnEvent\n// @Order(1)\nfunc (signal *SignalService) Echo(value SignalEcho) {\n\tsignal.Echoed = value.Size\n}\n";

        const SIGNAL_DRIVER: &str = "const { default: SignalService, __tachyon_event_plan__: plan } = await import('./SignalService.js')\n\
             const { default: CounterService } = await import('./CounterService.js')\n\
             const { default: ApplicationEvents } = await import('./application-events.mjs')\n\
             const events = []\n\
             const bus = new ApplicationEvents({ changed: () => {}, failed: () => { throw new Error('Event delivery failed') } })\n\
             for (const event of ['SignalSent', 'SignalEcho']) bus.attach({ receive: (value) => events.push([event, value]) }, { listeners: [{ method: 'receive', event, order: -100 }] }, {})\n\
             const signal = new SignalService({}), counter = new CounterService({ label: 'shared' })\n\
             bus.attach(signal, plan.types[0], { applicationEventPublisher: null })\n\
             await bus.initialize(signal)\n\
             signal.send(3); signal.send(8)\n\
             console.log(JSON.stringify({ large: signal.large, echoed: signal.echoed, events, label: counter.label }))\n\
             process.exit(0)\n";

        const GO_RETRY_CONFIGURATION: &str = "// @Setup\ntype RetrySetup struct{}\n\nfunc (RetrySetup) Configure(execution *BehaviorExecution) (*BehaviorConfiguration, error) {\n\treturn &BehaviorConfiguration{}, nil\n}\n";

        const GO_RETRY_SERVICE: &str = "// @Service\ntype RetryService struct {\n\tbehaviors *BehaviorContext\n\tCalls     int\n}\n\n// @OnStart\nfunc (retry *RetryService) Start() error {\n\tcontext, err := BehaviorsCreate[RetrySetup](NewBehaviorOptions(\"retry\", NewBehaviorSignal()))\n\tretry.behaviors = context\n\treturn err\n}\n\n// @Retry({\"maxRetries\":3,\"delayMs\":1,\"multiplier\":1,\"maxDelayMs\":5,\"jitterMs\":0})\nfunc (retry *RetryService) Flaky(value BehaviorValue) (BehaviorValue, error) {\n\tretry.Calls++\n\tif retry.Calls < 3 {\n\t\treturn nil, &BehaviorError{Code: \"adapter\", Message: \"transient\"}\n\t}\n\treturn value, nil\n}\n";

        const RETRY_DRIVER: &str = "const { default: RetryService } = await import('./RetryService.js')\n\
             const retry = new RetryService({})\n\
             const value = retry.flaky(3)\n\
             console.log(JSON.stringify({ value, calls: retry.calls }))\n\
             process.exit(0)\n";

        const FILE_FETCH: &str = "import { readFile } from 'node:fs/promises'\n\
             globalThis.fetch = async (url) => new Response(await readFile(url))\n";

        const TALLY_MODELS: &str = "public record TallyAdded(int Count);\n";

        const TALLY_SERVICE: &str = "[Service()]\n[Lifetime({\"value\":\"prototype\"})]\n[NeedsAll]\n\
             public class TallyService\n{\n    public int Count = 0;\n    public object Source;\n    \
             public ApplicationEventPublisher Publisher = new ApplicationEventPublisher();\n\n    \
             public int Size() => Source is object[] items ? items.Length : -1;\n\n    \
             public string Add(int by, string why)\n    {\n        Count += by;\n        \
             Publisher.PublishEvent(new Models.TallyAdded(Count));\n        return why + \" \" + Count;\n    }\n}\n";

        const FSHARP_TALLY_SERVICE: &str = "[<Service>]\ntype TallyService() =\n    member val Count = 0 with get, set\n\n    \
             member this.Add(by: int, why: string) : string =\n        this.Count <- this.Count + by\n        \
             why + \" \" + string this.Count\n";

        const FSHARP_NOTES_FILES: [(&str, &str); 2] = [
            (
                "client/setups/NotesSetup.fs",
                "[<Setup>]\ntype NotesSetup() =\n    static member configure(execution: BehaviorExecution) : BehaviorAdapters = BehaviorAdapters()\n",
            ),
            (
                "client/services/NotesService.fs",
                "open System.Threading.Tasks\n\n[<Service>]\n[<CacheSettings({\"name\":\"notes\",\"ttlMs\":60000,\"maxBytes\":4096,\"maxEntries\":8})>]\n\
                 type NotesService() =\n    let mutable reads = 0\n\n    \
                 member val behaviors : BehaviorContext = Behaviors.create(typeof<NotesSetup>, \"notes\", new BehaviorSignal()) with get\n\n    \
                 [<Cached>]\n    member this.find(key: BehaviorJSON) : Task<BehaviorJSON> =\n        reads <- reads + 1\n        \
                 Task.FromResult(BehaviorJSON.Object(dict [ \"key\", key; \"reads\", BehaviorJSON.From(reads) ]))\n\n    \
                 [<ClearCache({\"method\":\"find\"})>]\n    member this.forget(key: BehaviorJSON) : Task<BehaviorJSON> = Task.FromResult(key)\n\n    \
                 member this.Look(key: string) : string = this.find(BehaviorJSON.From(key)).Result.ToString()\n\n    \
                 member this.DropKey(key: string) : string = this.forget(BehaviorJSON.From(key)).Result.ToString()\n",
            ),
        ];

        const FSHARP_NOTES_DRIVER: &str = "console.info = () => {}\n\
             globalThis.indexedDB = { databases: async () => [], open: () => ({}) }\n\
             ;(0, eval)(await readFile('./tac-storage.js', 'utf8'))\n\
             const { default: NotesService } = await import('./NotesService.js')\n\
             const notes = new NotesService({})\n\
             notes.__tachyonStarted()\n\
             const first = JSON.parse(notes.look('a'))\n\
             const second = JSON.parse(notes.look('a'))\n\
             notes.dropKey('a')\n\
             const third = JSON.parse(notes.look('a'))\n\
             console.log(JSON.stringify({ first, second, third }))\n\
             process.exit(0)\n";

        const FSHARP_TALLY_DRIVER: &str = "import TallyService from './TallyService.js'\n\
             const one = new TallyService({})\n\
             one.__tachyonStarted()\n\
             const added = one.add(new Event('click'), 5, 'clicks')\n\
             console.log(JSON.stringify({ added, count: one.count }))\n\
             process.exit(0)\n";

        const TALLY_DRIVER: &str = "import TallyService from './TallyService.js'\n\
             const events = []\n\
             const one = new TallyService({ source: [1, 2, 3], ignored: true }), two = new TallyService({})\n\
             Object.defineProperty(one, '__tachyonPublisher', { value: { publishEvent: (event, value) => events.push([event, value]) } })\n\
             one.__tachyonStarted()\n\
             const added = one.add(new Event('click'), 5, 'clicks')\n\
             let bigStart = '', bigCall = ''\n\
             try { new TallyService({ source: 'x'.repeat(1100000) }) } catch (error) { bigStart = error.message }\n\
             try { one.add(new Event('click'), 1, 'x'.repeat(70000)) } catch (error) { bigCall = error.message }\n\
             console.log(JSON.stringify({ size: one.size(), other: two.size(), added, count: one.count, otherCount: two.count, events, bigStart, bigCall }))\n\
             process.exit(0)\n";
    }

    fn web_assembly_target_installed() -> bool {
        let Ok(output) = Command::new("rustc")
            .args([
                "--print",
                "target-libdir",
                "--target",
                crate::native::RustCompanion::WEB_TARGET,
            ])
            .output()
        else {
            return false;
        };
        if !output.status.success() {
            return false;
        }
        Path::new(String::from_utf8_lossy(&output.stdout).trim()).is_dir()
    }

    fn go_installed() -> bool {
        Command::new("go")
            .arg("version")
            .output()
            .is_ok_and(|output| output.status.success())
    }

    fn dotnet_web_workload_installed() -> bool {
        let Ok(output) = Command::new("dotnet").args(["workload", "list"]).output() else {
            return false;
        };
        String::from_utf8_lossy(&output.stdout).contains("wasm-tools")
    }

    fn node_available() -> bool {
        Command::new("node").arg("--version").output().is_ok()
    }

    fn write_service(root: &Path, relative: &str, source: &str) -> PathBuf {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(&path, source).expect("service");
        path
    }

    async fn build_and_publish(
        root: &Path,
        instance: &str,
        class: &str,
        language: WebLanguage,
        source: PathBuf,
    ) {
        let profile = crate::BuildProfile::Release;
        build_and_publish_with(root, (instance, class), language, source, profile).await;
    }

    async fn build_and_publish_with(
        root: &Path,
        (instance, class): (&str, &str),
        language: WebLanguage,
        source: PathBuf,
        profile: crate::BuildProfile,
    ) -> (Vec<String>, usize) {
        let services = [(
            String::from(instance),
            String::from(class),
            language,
            source,
        )];
        let published = build_all(root, &services, &super::BuildJobs::new(1), profile)
            .await
            .expect("build");
        let mut size = 0;
        for (name, bytes) in published.files {
            size += bytes.len();
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            std::fs::write(path, bytes).expect("publish");
        }
        (published.notes, size)
    }

    fn run_driver(root: &Path, script: &str) -> serde_json::Value {
        std::fs::write(root.join("drive.mjs"), script).expect("driver");
        let output = Command::new("node")
            .arg("drive.mjs")
            .current_dir(root)
            .output()
            .expect("node");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_str(stdout.trim()).expect("report")
    }

    #[tokio::test]
    async fn a_built_service_answers_from_its_webassembly_module() {
        if !web_assembly_target_installed() || !node_available() {
            eprintln!("skipping: the wasm32 Rust target or node is unavailable");
            return;
        }
        let root = tempfile::tempdir().expect("project");
        for (path, source) in Fixtures::COUNTER_MODELS {
            write_service(root.path(), path, source);
        }
        let source = write_service(
            root.path(),
            "client/services/CounterService.rs",
            Fixtures::COUNTER_SERVICE,
        );
        build_and_publish(
            root.path(),
            "counter",
            "CounterService",
            WebLanguage::Rust,
            source,
        )
        .await;
        let report = run_driver(root.path(), Fixtures::COUNTER_DRIVER);
        assert_counter_report(&report);
        assert_eq!(
            report["bigStart"],
            "Web service counter: The request is 1100115 bytes. The limit is 1048576 bytes. Pass less data to the service."
        );
        assert_eq!(
            report["bigCall"],
            "Web service counter: The request to add is 70121 bytes. The limit is 65536 bytes. Pass less data to the service."
        );
    }

    async fn rust_counter(profile: crate::BuildProfile) -> (Vec<String>, usize) {
        let root = tempfile::tempdir().expect("project");
        for (path, source) in Fixtures::COUNTER_MODELS {
            write_service(root.path(), path, source);
        }
        let source = write_service(
            root.path(),
            "client/services/CounterService.rs",
            Fixtures::COUNTER_SERVICE,
        );
        let service = ("counter", "CounterService");
        let built =
            build_and_publish_with(root.path(), service, WebLanguage::Rust, source, profile).await;
        assert_counter_report(&run_driver(root.path(), Fixtures::COUNTER_DRIVER));
        built
    }

    #[tokio::test]
    async fn a_release_rust_module_is_smaller_and_answers_like_a_development_one() {
        if !web_assembly_target_installed() || !node_available() {
            eprintln!("skipping: the wasm32 Rust target or node is unavailable");
            return;
        }
        let (development_notes, development) = rust_counter(crate::BuildProfile::Development).await;
        let (release_notes, release) = rust_counter(crate::BuildProfile::Release).await;
        assert!(development_notes.is_empty(), "{development_notes:?}");
        assert!(
            release_notes[0].starts_with("Rust: rustc tuned for size"),
            "{release_notes:?}"
        );
        if crate::wasm_optimizer::WasmOptimizer::is_installed() {
            assert!(
                release_notes[0].contains("wasm-opt -Oz"),
                "{release_notes:?}"
            );
            assert!(
                release < development,
                "release {release} >= development {development}"
            );
        }
    }

    async fn go_counter(root: &Path, profile: crate::BuildProfile) -> (Vec<String>, usize) {
        for (relative, model) in Fixtures::GO_COUNTER_MODELS {
            write_service(root, relative, model);
        }
        let source = write_service(
            root,
            "client/services/CounterService.go",
            Fixtures::GO_COUNTER_SERVICE,
        );
        let service = ("counter", "CounterService");
        let built = build_and_publish_with(root, service, WebLanguage::Go, source, profile).await;
        let driver = Fixtures::COUNTER_DRIVER.replace(
            "import CounterService, { __tachyon_event_plan__ as plan } from './CounterService.js'\n",
            "const { default: CounterService, __tachyon_event_plan__: plan } = await import('./CounterService.js')\n",
        );
        let script = format!("{}{driver}process.exit(0)\n", Fixtures::FILE_FETCH);
        assert_counter_report(&run_driver(root, &script));
        built
    }

    fn tinygo_installed() -> bool {
        Command::new("tinygo")
            .arg("version")
            .output()
            .is_ok_and(|output| output.status.success())
    }

    #[tokio::test]
    async fn a_release_go_module_is_built_by_tinygo_and_answers_like_a_development_one() {
        if !go_installed() || !tinygo_installed() || !node_available() {
            eprintln!("skipping: go, tinygo or node is unavailable");
            return;
        }
        let development_root = tempfile::tempdir().expect("project");
        let release_root = tempfile::tempdir().expect("project");
        write_service(
            release_root.path(),
            "client/manifest.json",
            "{\"tac\":{\"go\":{\"compiler\":\"tinygo\"}}}",
        );
        let (development_notes, development) =
            go_counter(development_root.path(), crate::BuildProfile::Development).await;
        let (release_notes, release) =
            go_counter(release_root.path(), crate::BuildProfile::Release).await;
        assert!(development_notes.is_empty(), "{development_notes:?}");
        assert!(
            release_notes[0].starts_with("Go: TinyGo"),
            "{release_notes:?}"
        );
        assert!(
            release * 3 < development,
            "release {release}, development {development}"
        );
        let glue =
            std::fs::read_to_string(release_root.path().join("go/wasm_exec.js")).expect("glue");
        assert!(
            glue.contains("wasi_snapshot_preview1"),
            "the glue must be TinyGo's own"
        );
    }

    #[tokio::test]
    async fn a_tinygo_service_compiles_a_regexp_with_long_repetitions() {
        if !go_installed() || !tinygo_installed() || !node_available() {
            eprintln!("skipping: go, tinygo or node is unavailable");
            return;
        }
        let root = tempfile::tempdir().expect("project");
        write_service(
            root.path(),
            "client/manifest.json",
            "{\"tac\":{\"go\":{\"compiler\":\"tinygo\"}}}",
        );
        let source = write_service(
            root.path(),
            "client/services/AddressService.go",
            "import \"regexp\"\n\n// @Service\ntype AddressService struct{}\n\n\
             func (service *AddressService) Valid(text string) bool {\n\t\
             return regexp.MustCompile(`^[a-z]{1,200}@[a-z]{1,200}$`).MatchString(text) && \
             regexp.MustCompile(`^[a-z0-9.]{0,58}@[a-z]{20,80}$`).MatchString(text+\"aaaaaaaaaaaaaaaaaaaa\")\n}\n",
        );
        let service = ("address", "AddressService");
        let profile = crate::BuildProfile::Release;
        build_and_publish_with(root.path(), service, WebLanguage::Go, source, profile).await;
        let driver = format!(
            "{}const {{ default: AddressService }} = await import('./AddressService.js')\n\
             const address = new AddressService({{}})\n\
             console.log(JSON.stringify({{ valid: await address.valid('ada@example') }}))\n\
             process.exit(0)\n",
            Fixtures::FILE_FETCH
        );
        assert_eq!(
            run_driver(root.path(), &driver),
            serde_json::json!({"valid": true})
        );
    }

    #[tokio::test]
    async fn the_go_compiler_setting_keeps_the_standard_compiler_in_a_release_build() {
        if !go_installed() || !node_available() {
            eprintln!("skipping: go or node is unavailable");
            return;
        }
        let root = tempfile::tempdir().expect("project");
        write_service(
            root.path(),
            "client/manifest.json",
            "{\"tac\":{\"go\":{\"compiler\":\"go\"}}}",
        );
        let (notes, _) = go_counter(root.path(), crate::BuildProfile::Release).await;
        assert!(
            notes[0].starts_with("Go: the standard Go compiler (tac.go.compiler is \"go\")"),
            "{notes:?}"
        );
    }

    fn assert_counter_report(report: &serde_json::Value) {
        assert_eq!(report["doubled"], 4);
        assert_eq!(report["after"], 41);
        assert_eq!(report["other"], 1);
        assert_eq!(report["label"], "many clicks");
        assert_eq!(report["otherLabel"], " clicks");
        assert_eq!(report["added"], "now 50");
        assert_eq!(report["stale"], true);
        assert_eq!(report["largeStart"], "");
        assert_eq!(report["keys"], serde_json::json!(["count", "label"]));
        assert_eq!(
            report["events"],
            serde_json::json!([
                ["one", "CounterStarted", {}],
                ["two", "CounterStarted", {}],
                ["one", "CounterAdded", {"count": 1}],
                ["one", "CounterAdded", {"count": 2}],
                ["two", "CounterAdded", {"count": 1}],
                ["one", "CounterAdded", {"count": 41}],
            ])
        );
        assert_eq!(report["plan"]["pre_destroy"], "__tachyonDispose");
    }

    #[tokio::test]
    async fn a_built_go_service_answers_from_its_webassembly_module() {
        if !go_installed() || !node_available() {
            assert!(
                std::env::var_os("CI").is_none(),
                "CI must provide go and node"
            );
            eprintln!("skipping: go or node is unavailable");
            return;
        }
        let root = tempfile::tempdir().expect("project");
        for (relative, model) in Fixtures::GO_COUNTER_MODELS {
            write_service(root.path(), relative, model);
        }
        let source = write_service(
            root.path(),
            "client/services/CounterService.go",
            Fixtures::GO_COUNTER_SERVICE,
        );
        build_and_publish(
            root.path(),
            "counter",
            "CounterService",
            WebLanguage::Go,
            source,
        )
        .await;
        let driver = Fixtures::COUNTER_DRIVER.replace(
            "import CounterService, { __tachyon_event_plan__ as plan } from './CounterService.js'\n",
            "const { default: CounterService, __tachyon_event_plan__: plan } = await import('./CounterService.js')\n",
        );
        let report = run_driver(
            root.path(),
            &format!("{}{driver}process.exit(0)\n", Fixtures::FILE_FETCH),
        );
        assert_counter_report(&report);
        assert_eq!(
            report["bigStart"],
            "Web service counter: The request is 1100115 bytes. The limit is 1048576 bytes. Pass less data to the service."
        );
        assert_eq!(
            report["bigCall"],
            "Web service counter: The request to add is 70121 bytes. The limit is 65536 bytes. Pass less data to the service."
        );
        assert!(root.path().join("go/services.wasm").is_file());
        let glue = std::fs::read_to_string(root.path().join("go/wasm_exec.js")).expect("glue");
        assert!(glue.contains("globalThis.Go = class"));
    }

    #[tokio::test]
    async fn a_go_service_reads_the_builtin_cache_and_store_synchronously() {
        if !go_installed() || !node_available() {
            assert!(
                std::env::var_os("CI").is_none(),
                "CI must provide go and node"
            );
            eprintln!("skipping: go or node is unavailable");
            return;
        }
        let root = tempfile::tempdir().expect("project");
        let mut source = PathBuf::new();
        for (relative, text) in Fixtures::GO_STORAGE_FILES {
            source = write_service(root.path(), relative, text);
        }
        std::fs::write(
            root.path().join("tac-storage.js"),
            include_str!("template/tac-storage.js"),
        )
        .expect("storage runtime");
        build_and_publish(
            root.path(),
            "notes",
            "NotesService",
            WebLanguage::Go,
            source,
        )
        .await;
        let report = run_driver(
            root.path(),
            &format!("{}{}", Fixtures::FILE_FETCH, Fixtures::STORAGE_DRIVER),
        );
        assert_eq!(report["first"], serde_json::json!({"key": "a", "reads": 1}));
        assert_eq!(report["second"], report["first"]);
        assert_eq!(report["third"], serde_json::json!({"key": "a", "reads": 2}));
        assert!(
            report["refused"]
                .as_str()
                .is_some_and(|message| message.contains("the note was refused")),
            "{report}"
        );
        assert_eq!(
            report["listed"],
            serde_json::json!([{"id": "n1", "text": "hello"}])
        );
    }

    #[tokio::test]
    async fn a_rust_service_reads_the_builtin_cache_and_store_synchronously() {
        if !web_assembly_target_installed() || !node_available() {
            eprintln!("skipping: the wasm32 Rust target or node is unavailable");
            return;
        }
        let root = tempfile::tempdir().expect("project");
        let mut source = PathBuf::new();
        for (relative, text) in Fixtures::RUST_STORAGE_FILES {
            source = write_service(root.path(), relative, text);
        }
        std::fs::write(
            root.path().join("tac-storage.js"),
            include_str!("template/tac-storage.js"),
        )
        .expect("storage runtime");
        build_and_publish(
            root.path(),
            "notes",
            "NotesService",
            WebLanguage::Rust,
            source,
        )
        .await;
        let report = run_driver(
            root.path(),
            &format!("{}{}", Fixtures::FILE_FETCH, Fixtures::RUST_STORAGE_DRIVER),
        );
        assert_eq!(report["first"], serde_json::json!({"key": "a", "reads": 1}));
        assert_eq!(report["second"], report["first"]);
        assert_eq!(report["third"], serde_json::json!({"key": "a", "reads": 2}));
        assert!(
            report["refused"]
                .as_str()
                .is_some_and(|message| message.contains("the note was refused")),
            "{report}"
        );
        assert_eq!(
            report["listed"],
            serde_json::json!([{"id": "n1", "text": "hello"}])
        );
    }

    #[tokio::test]
    async fn go_services_share_one_module_and_their_listeners_follow_conditions() {
        if !go_installed() || !node_available() {
            assert!(
                std::env::var_os("CI").is_none(),
                "CI must provide go and node"
            );
            eprintln!("skipping: go or node is unavailable");
            return;
        }
        let root = tempfile::tempdir().expect("project");
        for (relative, model) in Fixtures::GO_COUNTER_MODELS
            .iter()
            .chain(Fixtures::GO_SIGNAL_MODELS.iter())
        {
            write_service(root.path(), relative, model);
        }
        let mut services = Vec::new();
        for (instance, class, source) in [
            ("counter", "CounterService", Fixtures::GO_COUNTER_SERVICE),
            ("signal", "SignalService", Fixtures::GO_SIGNAL_SERVICE),
        ] {
            let relative = format!("client/services/{class}.go");
            let path = write_service(root.path(), &relative, source);
            services.push((
                String::from(instance),
                String::from(class),
                WebLanguage::Go,
                path,
            ));
        }
        let jobs = super::BuildJobs::new(1);
        let profile = crate::BuildProfile::Release;
        let built = build_all(root.path(), &services, &jobs, profile).await;
        for (name, bytes) in built.expect("build").files {
            let path = root.path().join(name);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            std::fs::write(path, bytes).expect("publish");
        }
        let driver = format!("{}{}", Fixtures::FILE_FETCH, Fixtures::SIGNAL_DRIVER);
        std::fs::write(
            root.path().join("application-events.mjs"),
            concat!(
                include_str!("template/application-events.js"),
                "\nexport default TacApplicationEvents\n"
            ),
        )
        .expect("event runtime");
        let report = run_driver(root.path(), &driver);
        assert_eq!(report["large"], serde_json::json!([8]));
        assert_eq!(report["echoed"], 80);
        assert_eq!(report["label"], "shared clicks");
        assert_eq!(
            report["events"],
            serde_json::json!([
                ["SignalSent", {"size": 3}],
                ["SignalSent", {"size": 8}],
                ["SignalEcho", {"size": 80}],
            ])
        );
    }

    #[tokio::test]
    async fn a_go_web_service_runs_its_behavior_annotations() {
        if !go_installed() || !node_available() {
            assert!(
                std::env::var_os("CI").is_none(),
                "CI must provide go and node"
            );
            eprintln!("skipping: go or node is unavailable");
            return;
        }
        let root = tempfile::tempdir().expect("project");
        write_service(
            root.path(),
            "client/setups/RetrySetup.go",
            Fixtures::GO_RETRY_CONFIGURATION,
        );
        let source = write_service(
            root.path(),
            "client/services/RetryService.go",
            Fixtures::GO_RETRY_SERVICE,
        );
        build_and_publish(
            root.path(),
            "retry",
            "RetryService",
            WebLanguage::Go,
            source,
        )
        .await;
        let driver = format!("{}{}", Fixtures::FILE_FETCH, Fixtures::RETRY_DRIVER);
        let report = run_driver(root.path(), &driver);
        assert_eq!(report, serde_json::json!({"value": 3, "calls": 3}));
    }

    #[tokio::test]
    async fn a_built_csharp_service_answers_from_the_dotnet_runtime() {
        if !dotnet_web_workload_installed() || !node_available() {
            eprintln!("skipping: the .NET wasm-tools workload or node is unavailable");
            return;
        }
        for profile in [
            crate::BuildProfile::Release,
            crate::BuildProfile::Development,
        ] {
            let root = tempfile::tempdir().expect("project");
            write_service(
                root.path(),
                "client/models/TallyAdded.cs",
                Fixtures::TALLY_MODELS,
            );
            let source = write_service(
                root.path(),
                "client/services/TallyService.cs",
                Fixtures::TALLY_SERVICE,
            );
            let service = ("tally", "TallyService");
            let (notes, _) =
                build_and_publish_with(root.path(), service, WebLanguage::CSharp, source, profile)
                    .await;
            let explained = notes
                .iter()
                .any(|note| note.starts_with("C#: the trimmed .NET interpreter"));
            assert_eq!(
                explained,
                profile == crate::BuildProfile::Release,
                "{notes:?}"
            );
            let report = run_driver(root.path(), Fixtures::TALLY_DRIVER);
            assert_eq!(report["size"], 3, "{profile:?}");
            assert_eq!(report["other"], -1);
            assert_eq!(report["added"], "clicks 5");
            assert_eq!(report["count"], 5);
            assert_eq!(report["otherCount"], 0);
            assert_eq!(
                report["events"],
                serde_json::json!([["TallyAdded", {"count": 5}]])
            );
            assert_eq!(
                report["bigStart"],
                "Web service tally: The request is 1100114 bytes. The limit is 1048576 bytes. Pass less data to the service."
            );
            assert_eq!(
                report["bigCall"],
                "Web service tally: The request to add is 70119 bytes. The limit is 65536 bytes. Pass less data to the service."
            );
        }
    }

    #[tokio::test]
    async fn a_built_fsharp_service_answers_from_the_dotnet_runtime() {
        if !dotnet_web_workload_installed() || !node_available() {
            eprintln!("skipping: the .NET wasm-tools workload or node is unavailable");
            return;
        }
        let root = tempfile::tempdir().expect("project");
        let source = write_service(
            root.path(),
            "client/services/TallyService.fs",
            Fixtures::FSHARP_TALLY_SERVICE,
        );
        build_and_publish(
            root.path(),
            "tally",
            "TallyService",
            WebLanguage::CSharp,
            source,
        )
        .await;
        let report = run_driver(root.path(), Fixtures::FSHARP_TALLY_DRIVER);
        assert_eq!(report["added"], "clicks 5");
        assert_eq!(report["count"], 5);
    }

    #[tokio::test]
    async fn an_fsharp_service_caches_through_the_page_storage_host() {
        if !dotnet_web_workload_installed() || !node_available() {
            eprintln!("skipping: the .NET wasm-tools workload or node is unavailable");
            return;
        }
        let root = tempfile::tempdir().expect("project");
        let mut source = PathBuf::new();
        for (relative, text) in Fixtures::FSHARP_NOTES_FILES {
            source = write_service(root.path(), relative, text);
        }
        std::fs::write(
            root.path().join("tac-storage.js"),
            include_str!("template/tac-storage.js"),
        )
        .expect("storage runtime");
        build_and_publish(
            root.path(),
            "notes",
            "NotesService",
            WebLanguage::CSharp,
            source,
        )
        .await;
        let report = run_driver(
            root.path(),
            &format!("{}{}", Fixtures::FILE_FETCH, Fixtures::FSHARP_NOTES_DRIVER),
        );
        assert_eq!(report["first"], serde_json::json!({"key": "a", "reads": 1}));
        assert_eq!(report["second"], report["first"]);
        assert_eq!(report["third"], serde_json::json!({"key": "a", "reads": 2}));
    }

    #[test]
    fn the_job_limit_comes_from_the_setting_or_the_core_count() {
        assert_eq!(super::BuildJobs::limit(Some("6"), 10), 6);
        assert_eq!(super::BuildJobs::limit(Some(" 1 "), 10), 1);
        assert_eq!(super::BuildJobs::limit(Some("0"), 10), 3);
        assert_eq!(super::BuildJobs::limit(Some("many"), 10), 3);
        assert_eq!(super::BuildJobs::limit(None, 2), 1);
        assert_eq!(super::BuildJobs::limit(None, 32), 4);
    }

    #[tokio::test]
    async fn build_jobs_run_in_parallel_up_to_the_limit_and_keep_their_order() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let jobs = super::BuildJobs::new(2);
        let running = AtomicUsize::new(0);
        let most_running = AtomicUsize::new(0);
        let mut work = Vec::new();
        for index in 0..5 {
            let running = &running;
            let most_running = &most_running;
            work.push(jobs.run(async move {
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                most_running.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(40)).await;
                running.fetch_sub(1, Ordering::SeqCst);
                index
            }));
        }
        let finished = futures_util::future::join_all(work).await;
        assert_eq!(finished, vec![0, 1, 2, 3, 4]);
        assert_eq!(most_running.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn encodes_base64_with_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(&[0xfb, 0xff]), "+/8=");
    }

    #[test]
    fn a_service_module_addresses_its_instance() {
        let topics = serde_json::json!({ "ClockTicked": "/.tachyon/topics/clock-ticked" });
        let source = module("clock", "ClockService", "tachyon-rust.js", &topics);
        assert!(source.contains("export default class ClockService {"));
        assert!(source.contains("\"topics\":{\"ClockTicked\":\"/.tachyon/topics/clock-ticked\"}"));
        assert!(
            source
                .contains("activate(this, \"clock\", properties, __tachyon_event_plan__.types[0])")
        );
        assert!(source.contains("\"pre_destroy\":\"__tachyonDispose\""));
    }
}

#[cfg(test)]
mod document_title_tests;

#[cfg(test)]
mod dotnet_http_host_tests;
