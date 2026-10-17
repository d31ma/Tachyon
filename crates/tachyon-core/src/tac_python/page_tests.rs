use super::runtime_assets::RuntimeAssets;
use std::path::{Path, PathBuf};

struct PythonPage;

impl PythonPage {
    const MANIFEST: &'static str =
        "{\"tac\":{\"python\":{\"runtime\":\"pyodide-314.0.7\",\"profile\":\"offline\"}}}\n";

    const FILE_FETCH: &'static str = "import { readFile, writeFile } from 'node:fs/promises'\nimport { fileURLToPath } from 'node:url'\nglobalThis.fetch = async (url) => new Response(await readFile(fileURLToPath(url)))\nglobalThis.addEventListener = () => {}\nglobalThis.removeEventListener = () => {}\nconst engine = await readFile('./tachyon-python-services.js', 'utf8')\nawait writeFile('./tachyon-python-services.js', engine.replace(/(new URL\\('[^']+', root\\)|root)\\.href/g, 'decodeURIComponent($1.pathname)'))\n";

    const STORAGE_FILES: [(&'static str, &'static str); 2] = [
        (
            "client/setups/BrowserSetup.py",
            "@setup\nclass BrowserSetup:\n    @staticmethod\n    def configure(execution):\n        return {}\n",
        ),
        (
            "client/entities/NoteEntity.py",
            "from typing import Annotated\n\n@entity({\"adapter\":\"indexeddb\"})\n@table(\"notes\")\nclass NoteEntity:\n    id: Annotated[str, id()] = ''\n    text: str = ''\n",
        ),
    ];

    const NOTES: &'static str = "import asyncio\nfrom typing import Annotated\n\n@service\n@cache_settings({\"name\":\"notes\",\"ttlMs\":60000,\"maxBytes\":4096,\"maxEntries\":8})\nclass NotesService:\n    reads: int = 0\n    attempts: int = 0\n\n    @on_start\n    async def start(self) -> None:\n        self.behaviors = await Behaviors.create(BrowserSetup, {'partition': 'notes', 'signal': BehaviorSignal()})\n\n    @cached\n    async def find(self, key: str) -> int:\n        self.reads += 1\n        return self.reads\n\n    @clear_cache({\"method\":\"find\"})\n    async def forget(self, key: str) -> bool:\n        return True\n\n    @retry({\"maxRetries\":3,\"delayMs\":1,\"multiplier\":1,\"maxDelayMs\":5,\"jitterMs\":0})\n    async def steady(self) -> int:\n        self.attempts += 1\n        if self.attempts < 3:\n            raise RuntimeError('not yet')\n        return self.attempts\n\n    @max_at_once({\"limit\":1,\"policy\":\"reject\"})\n    async def single(self, wait: int) -> int:\n        await asyncio.sleep(wait / 1000)\n        return wait\n\n    async def save(self, id: str, text: str) -> bool:\n        await self.behaviors.repository(NoteEntity).save({'id': id, 'text': text})\n        return True\n\n    @all_or_nothing({\"adapter\":\"indexeddb\"})\n    async def save_then_refuse(self, id: str) -> bool:\n        await self.save(id, 'draft')\n        raise BehaviorError('validation', 'the note was refused')\n\n    async def count(self) -> int:\n        return len(await self.behaviors.repository(NoteEntity).find({}))\n\n    def plain(self) -> int:\n        return self.reads\n\n    @cached\n    async def priced(self, sku: Annotated[str, cache_key], note: str) -> int:\n        self.reads += 1\n        return self.reads\n\n    @update_cache({\"method\":\"find\"})\n    async def write(self, key: str) -> int:\n        return 99\n\n    def page_size(self, page: Annotated[dict[str, int], page_defaults({\"defaultSize\":5,\"maxSize\":10,\"sort\":[\"id\"]})]) -> int:\n        return page['size']\n";

    const NOTES_DRIVER: &'static str = "console.info = () => {}\n\
         globalThis.indexedDB = { databases: async () => [], open: () => ({}) }\n\
         ;(0, eval)(await readFile('./tac-storage.js', 'utf8'))\n\
         const { default: NotesService } = await import('./NotesService.js')\n\
         const notes = new NotesService({})\n\
         const early = await notes.plain()\n\
         const later = notes.plain()\n\
         const first = await notes.find('a')\n\
         const second = await notes.find('a')\n\
         await notes.forget('a')\n\
         const third = await notes.find('a')\n\
         const retried = await notes.steady()\n\
         const [held, rejected] = await Promise.allSettled([notes.single(20), notes.single(1)])\n\
         await notes.save('n1', 'hello')\n\
         let refused = ''\n\
         try { await notes.saveThenRefuse('n2') } catch (error) { refused = error.message }\n\
         const saved = await notes.count()\n\
         const priced = [await notes.priced('anvil', 'one'), await notes.priced('anvil', 'two')]\n\
         await notes.write('a')\n\
         const written = await notes.find('a')\n\
         const size = notes.pageSize({})\n\
         const pending = notes.single(50)\n\
         notes.__tachyonDispose()\n\
         let cancelled = ''\n\
         try { await pending } catch (error) { cancelled = error.code }\n\
         console.log(JSON.stringify({ early, later, first, second, third, retried, held: held.value, rejected: rejected.status, refused, saved, priced, written, size, cancelled }))\n\
         process.exit(0)\n";

    fn skip_reason() -> Option<String> {
        let node = std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success());
        if !node {
            return Some(String::from("node is unavailable"));
        }
        RuntimeAssets::read(&RuntimeAssets::cached_root())
            .err()
            .map(|_| String::from("the pinned Pyodide runtime is not in the Tachyon cache"))
    }

    fn write(root: &Path, relative: &str, text: &str) -> PathBuf {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap_or(root))
            .unwrap_or_else(|error| panic!("directory: {error}"));
        std::fs::write(&path, text).unwrap_or_else(|error| panic!("source: {error}"));
        path
    }

    async fn publish(root: &Path, services: &[(&str, &str, &str)]) {
        Self::write(root, "client/manifest.json", Self::MANIFEST);
        let chosen = services
            .iter()
            .map(|(instance, class, source)| {
                let path = Self::write(root, &format!("client/services/{class}.py"), source);
                (
                    String::from(*instance),
                    String::from(*class),
                    crate::tac_web::WebLanguage::Python,
                    path,
                )
            })
            .collect::<Vec<_>>();
        let jobs = crate::tac_web::BuildJobs::new(1);
        let published =
            crate::tac_web::build_all(root, &chosen, &jobs, crate::BuildProfile::Release)
                .await
                .unwrap_or_else(|error| panic!("build: {error:?}"));
        for (name, bytes) in published.files {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap_or(root))
                .unwrap_or_else(|error| panic!("directory: {error}"));
            std::fs::write(path, bytes).unwrap_or_else(|error| panic!("publish: {error}"));
        }
    }

    fn run(root: &Path, driver: &str) -> serde_json::Value {
        Self::write(root, "drive.mjs", &format!("{}{driver}", Self::FILE_FETCH));
        let output = std::process::Command::new("node")
            .arg("drive.mjs")
            .current_dir(root)
            .output()
            .unwrap_or_else(|error| panic!("node: {error}"));
        let printed = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{printed}{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let last = printed.lines().last().unwrap_or_default();
        serde_json::from_str(last).unwrap_or_else(|error| panic!("{error}: {printed}"))
    }
}

#[tokio::test]
async fn python_async_methods_use_the_page_cache_store_and_resilience() {
    if let Some(reason) = PythonPage::skip_reason() {
        eprintln!("skipping: {reason}");
        return;
    }
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("project: {error}"));
    for (relative, text) in PythonPage::STORAGE_FILES {
        PythonPage::write(root.path(), relative, text);
    }
    PythonPage::write(
        root.path(),
        "tac-storage.js",
        include_str!("../template/tac-storage.js"),
    );
    PythonPage::publish(root.path(), &[("notes", "NotesService", PythonPage::NOTES)]).await;
    let report = PythonPage::run(root.path(), PythonPage::NOTES_DRIVER);
    assert_eq!(report["early"], 0, "{report}");
    assert_eq!(report["later"], 0, "{report}");
    assert_eq!(report["first"], 1, "{report}");
    assert_eq!(report["second"], 1, "{report}");
    assert_eq!(report["third"], 2, "{report}");
    assert_eq!(report["retried"], 3, "{report}");
    assert_eq!(report["held"], 20, "{report}");
    assert_eq!(report["rejected"], "rejected", "{report}");
    assert_eq!(
        report["refused"], "Web service notes: the note was refused",
        "{report}"
    );
    assert_eq!(report["saved"], 1, "{report}");
    assert_eq!(report["priced"], serde_json::json!([3, 3]), "{report}");
    assert_eq!(report["written"], 99, "{report}");
    assert_eq!(report["size"], 5, "{report}");
    assert_eq!(report["cancelled"], "CANCELLED", "{report}");
}

struct PythonContracts;

impl PythonContracts {
    const FILES: [(&'static str, &'static str); 2] = [
        (
            "client/stores/NotesStore.py",
            "from typing import Protocol\n\n@store(NoteEntity, {\"fields\":[\"id\",\"text\"],\"defaultSize\":20,\"maxSize\":100,\"setup\":\"BrowserSetup\"})\nclass NotesStore(Repository[NoteEntity, str], Protocol):\n    async def find_by_text(self, text: str) -> list[dict[str, str]]: ...\n    @query({\"operation\":\"count\",\"where\":[],\"orderBy\":[]})\n    async def total(self) -> int: ...\n",
        ),
        (
            "client/remotes/CatalogClient.py",
            "from typing import Annotated, Protocol\n\n@http_client({\"path\":\"/api\",\"setup\":\"BrowserSetup\"})\nclass CatalogClient(Protocol):\n    @get_request(\"/items/{id}\")\n    async def find(self, id: Annotated[int, path_variable], tag: Annotated[str, request_param(\"tag\")]) -> dict[str, object]: ...\n    @post_request(\"/items\")\n    async def create(self, item: Annotated[dict[str, object], request_body], key: Annotated[str, request_header(\"X-Key\")]) -> dict[str, object]: ...\n",
        ),
    ];

    const LIBRARY: &'static str = "from typing import Annotated\n\n@service\nclass LibraryService:\n    notes_store: Annotated[NotesStore, needs]\n    catalog_client: Annotated[CatalogClient, needs]\n\n    async def add(self, id: str, text: str) -> int:\n        await self.notes_store.save({'id': id, 'text': text})\n        return await self.notes_store.total()\n\n    async def matching(self, text: str) -> int:\n        return len(await self.notes_store.find_by_text(text))\n\n    async def item(self, id: int) -> str:\n        answer = await self.catalog_client.find(id, 'new')\n        return answer['path']\n\n    async def create(self, name: str) -> str:\n        answer = await self.catalog_client.create({'name': name}, 'k1')\n        return answer['method'] + ' ' + answer['body'] + ' ' + answer['key']\n";

    const DRIVER: &'static str = "console.info = () => {}\n\
         globalThis.indexedDB = { databases: async () => [], open: () => ({}) }\n\
         ;(0, eval)(await readFile('./tac-storage.js', 'utf8'))\n\
         globalThis[Symbol.for('tachyon.tac.http')] = { start: (text) => {\n\
         \x20 const request = JSON.parse(text)\n\
         \x20 const path = `${request.path}?tag=${request.query.tag ?? ''}`\n\
         \x20 const value = { path, method: request.method, body: JSON.stringify(request.body ?? null), key: request.headers['X-Key'] ?? '' }\n\
         \x20 return { answer: Promise.resolve(JSON.stringify({ value })), cancel() {} }\n\
         } }\n\
         const { default: LibraryService } = await import('./LibraryService.js')\n\
         const library = new LibraryService({})\n\
         const first = await library.add('n1', 'hello')\n\
         const second = await library.add('n2', 'bye')\n\
         const matching = await library.matching('hello')\n\
         const item = await library.item(7)\n\
         const created = await library.create('anvil')\n\
         console.log(JSON.stringify({ first, second, matching, item, created }))\n\
         process.exit(0)\n";
}

#[tokio::test]
async fn python_needs_a_store_and_an_http_client_from_their_contracts() {
    if let Some(reason) = PythonPage::skip_reason() {
        eprintln!("skipping: {reason}");
        return;
    }
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("project: {error}"));
    for (relative, text) in PythonPage::STORAGE_FILES
        .into_iter()
        .chain(PythonContracts::FILES)
    {
        PythonPage::write(root.path(), relative, text);
    }
    PythonPage::write(
        root.path(),
        "tac-storage.js",
        include_str!("../template/tac-storage.js"),
    );
    PythonPage::publish(
        root.path(),
        &[("library", "LibraryService", PythonContracts::LIBRARY)],
    )
    .await;
    let report = PythonPage::run(root.path(), PythonContracts::DRIVER);
    assert_eq!(report["first"], 1, "{report}");
    assert_eq!(report["second"], 2, "{report}");
    assert_eq!(report["matching"], 1, "{report}");
    assert_eq!(report["item"], "/api/items/7?tag=new", "{report}");
    assert_eq!(
        report["created"], "POST {\"name\":\"anvil\"} k1",
        "{report}"
    );
}
