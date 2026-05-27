// @ts-check

const BROWSER_PATH = "__FYLO_BROWSER_PATH__";

// ── Inline FYLO global bootstrap ───────────────────────────────────────────
// This file is served as raw text (with __FYLO_BROWSER_PATH__ string-replaced
// at runtime by FyloBrowser.registerRoutes), so it cannot use ES imports. The
// framework runtime (fylo-global.js) provides the same client for companion
// scripts; this inline copy is only used by the standalone /_fylo shell.
if (!/** @type {any} */ (window).fylo) {
    class InlineFyloCollectionClient {
        /**
         * @param {InlineFyloBrowserClient} browserClient
         * @param {string} collection
         */
        constructor(browserClient, collection) {
            this.browserClient = browserClient;
            this.collection = collection;
        }

        /**
         * Query the collection using PostgREST-style filters.
         * e.g. find({ role: "eq.admin", age: "gt.18", select: "name,role", order: "name.asc", limit: 10 })
         * @param {Record<string, unknown>} [query]
         */
        async find(query = {}) {
            const params = new URLSearchParams();
            for (const [key, value] of Object.entries(query)) {
                params.set(key, String(value));
            }
            const qs = params.toString();
            const url = `/${encodeURIComponent(this.collection)}/${qs ? `?${qs}` : ""}`;
            const response = await this.browserClient.fetch(url);
            return response.json();
        }

        /** @param {number} [limit] */
        async list(limit = 25) {
            const response = await this.browserClient.fetch(`/${encodeURIComponent(this.collection)}/?limit=${limit}`);
            return response.json();
        }

        /** @param {string} id */
        async get(id) {
            const response = await this.browserClient.fetch(`/${encodeURIComponent(this.collection)}/${encodeURIComponent(id)}/`);
            return response.json();
        }

        /** @param {number} [since] */
        async events(since = 0) {
            const response = await this.browserClient.fetch(`/api/events?collection=${encodeURIComponent(this.collection)}&since=${since}`);
            return response.json();
        }

        /** @param {Record<string, unknown>} doc */
        async create(doc) {
            return this.browserClient.postJson(`/${encodeURIComponent(this.collection)}/`, doc);
        }

        /** @param {string} id @param {Record<string, unknown>} doc */
        async put(id, doc) {
            const response = await this.browserClient.fetch(`/${encodeURIComponent(this.collection)}/${encodeURIComponent(id)}/`, {
                method: "PUT",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(doc),
            });
            return response.json();
        }

        /** @param {string} id @param {Record<string, unknown>} doc */
        async patch(id, doc) {
            const response = await this.browserClient.fetch(`/${encodeURIComponent(this.collection)}/${encodeURIComponent(id)}/`, {
                method: "PATCH",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(doc),
            });
            return response.json();
        }

        /** @param {string} id */
        async del(id) {
            const response = await this.browserClient.fetch(`/${encodeURIComponent(this.collection)}/${encodeURIComponent(id)}/`, { method: "DELETE" });
            if (response.status === 204) return { ok: true };
            return response.json();
        }

        async rebuild() {
            return this.browserClient.postJson("/api/rebuild", { collection: this.collection });
        }
    }

    class InlineFyloBrowserClient {
        constructor() {
            /** @type {string | null} */
            this.authHeader = null;
            this.state = this.createState();
            this.proxy = this.createProxy();
        }

        /**
         * @param {string} user
         * @param {string} pass
         */
        basicAuth(user, pass) {
            const bytes = new TextEncoder().encode(`${user}:${pass}`);
            let binary = "";
            for (const byte of bytes) binary += String.fromCharCode(byte);
            return `Basic ${btoa(binary)}`;
        }

        /** @returns {Record<string, unknown>} */
        createState() {
            return {
                enabled: false,
                /** @type {string | undefined} */
                root: undefined,
                setCredentials: this.setCredentials.bind(this),
                clearCredentials: this.clearCredentials.bind(this),
                collections: this.collections.bind(this),
                meta: this.meta.bind(this),
                request: this.request.bind(this),
            };
        }

        createProxy() {
            return /** @type {any} */ (new Proxy(this.state, {
                get: (target, prop) => {
                    if (typeof prop !== "string") return Reflect.get(target, prop);
                    if (prop in target) return Reflect.get(target, prop);
                    return new InlineFyloCollectionClient(this, prop);
                },
                set(target, prop, value) {
                    return Reflect.set(target, prop, value);
                },
                has(target, prop) {
                    return typeof prop === "string" || prop in target;
                },
            }));
        }

    // Probe meta once — if the browser route is mounted, flip enabled and cache the root.
        /**
         * @param {string} user
         * @param {string} pass
         */
        setCredentials(user, pass) {
            this.authHeader = this.basicAuth(user, pass);
        }

        clearCredentials() {
            this.authHeader = null;
        }

        /**
         * @param {string} path
         * @param {RequestInit} [init]
         */
        fetch(path, init = {}) {
            const headers = new Headers(init.headers || {});
            if (this.authHeader) headers.set("Authorization", this.authHeader);
            return fetch(`${BROWSER_PATH}${path}`, { ...init, headers });
        }

        /**
         * @param {string} path
         * @param {RequestInit} [init]
         */
        request(path, init = {}) {
            if (/^https?:\/\//i.test(path)) {
                const headers = new Headers(init.headers || {});
                if (this.authHeader) headers.set("Authorization", this.authHeader);
                return fetch(path, { ...init, headers });
            }
            const relativePath = path.startsWith(BROWSER_PATH) ? path.slice(BROWSER_PATH.length) || "/" : path;
            return this.fetch(relativePath, init);
        }

        /**
         * @param {string} path
         * @param {Record<string, unknown>} body
         */
        async postJson(path, body) {
            const response = await this.fetch(path, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(body),
            });
            return response.json();
        }

        async collections() {
            const response = await this.fetch("/api/collections", { cache: "reload" });
            if (!response.ok) return { root: "", collections: [] };
            const collectionsPayload = await response.json();
            this.state.root = collectionsPayload.root;
            return collectionsPayload;
        }

        async meta() {
            const response = await this.fetch("/api/meta", { cache: "reload" });
            if (!response.ok) return null;
            return response.json();
        }

        async probe() {
            try {
                const meta = await this.meta();
                if (meta) {
                    this.state.enabled = true;
                    this.state.root = meta.root;
                }
            } catch {
                /* fylo browser not mounted; leave disabled */
            }
        }
    }

    const inlineFyloClient = new InlineFyloBrowserClient();
    /** @type {any} */ (window).fylo = inlineFyloClient.proxy;
    void inlineFyloClient.probe();
}

/** @type {any} */
const fylo = /** @type {any} */ (window).fylo;

// ── DOM references ─────────────────────────────────────────────────────────

/** @type {HTMLElement | null} */
const collectionsRoot = document.querySelector("#fylo-collections");
/** @type {HTMLElement | null} */
const documentsRoot = document.querySelector("#fylo-documents");
/** @type {HTMLElement | null} */
const collectionLabel = document.querySelector("#fylo-collection-name");
/** @type {HTMLElement | null} */
const rootLabel = document.querySelector("#fylo-root");
/** @type {HTMLElement | null} */
const detailRoot = document.querySelector("#fylo-detail");
/** @type {HTMLElement | null} */
const detailIdLabel = document.querySelector("#fylo-detail-id");
/** @type {HTMLElement | null} */
const queryModeChip = document.querySelector("#fylo-query-mode");
/** @type {HTMLElement | null} */
const queryToggleBtn = document.querySelector("#fylo-query-toggle");
/** @type {HTMLElement | null} */
const queryRunBtn = document.querySelector("#fylo-query-run");
/** @type {HTMLSelectElement | null} */
const requestMethodField = document.querySelector("#fylo-request-method");
/** @type {HTMLInputElement | null} */
const requestPathField = document.querySelector("#fylo-request-path");
/** @type {HTMLTextAreaElement | null} */
const queryField = document.querySelector("#fylo-query-source");
/** @type {HTMLElement | null} */
const queryResultsRoot = document.querySelector("#fylo-query-results");
/** @type {HTMLElement | null} */
const eventsRoot = document.querySelector("#fylo-events");
/** @type {HTMLElement | null} */
const eventsStatusChip = document.querySelector("#fylo-events-status");
/** @type {HTMLElement | null} */
const eventsToggleBtn = document.querySelector("#fylo-events-toggle");
/** @type {HTMLElement | null} */
const eventsClearBtn = document.querySelector("#fylo-events-clear");

const EVENTS_POLL_MS = 3000;
const EVENTS_MAX_ROWS = 200;

const state = {
    selected: /** @type {string | null} */ (null),
    selectedDocId: /** @type {string | null} */ (null),
    /** @type {ReturnType<typeof setInterval> | null} */
    eventsTimer: null,
    /** @type {string | null} */
    eventsCollection: null,
    eventsOffset: 0,
};

/**
 * @param {HTMLElement | null} chip
 * @param {string} label
 */
function setChipLabel(chip, label) {
    if (!chip) return;
    chip.textContent = label;
}

/**
 * @param {HTMLElement | null} target
 * @param {string} message
 */
function renderError(target, message) {
    if (!target) return;
    const node = document.createElement("div");
    node.className = "fylo-error";
    node.textContent = message;
    target.replaceChildren(node);
}

/**
 * @param {unknown} error
 * @returns {string}
 */
function errorMessage(error) {
    if (error instanceof Error) return error.message;
    return String(error);
}

async function loadMeta() {
    try {
        const meta = await fylo.meta();
        if (!meta) {
            setChipLabel(rootLabel, "root: unknown");
            return;
        }
        setChipLabel(rootLabel, `${meta.root}${meta.readOnly ? " · read-only" : ""}`);
    } catch {
        setChipLabel(rootLabel, "root: unknown");
    }
}

async function loadCollections() {
    if (!collectionsRoot) return;
    try {
        const collectionsResponse = await fylo.collections();
        if (!collectionsResponse.collections.length) {
            collectionsRoot.replaceChildren(
                Object.assign(document.createElement("p"), {
                    className: "muted md-typescale-body-medium",
                    textContent: "No collections found at this root yet.",
                }),
            );
            return;
        }
        collectionsRoot.replaceChildren();
        for (const collection of collectionsResponse.collections) {
            const item = document.createElement("div");
            item.className = "fylo-collection";
            item.setAttribute("role", "button");
            item.setAttribute("tabindex", "0");
            item.setAttribute("aria-selected", "false");
            item.dataset.collection = collection.name;

            const name = document.createElement("span");
            name.className = "fylo-collection-name md-typescale-title-medium";
            name.textContent = collection.name;

            const meta = document.createElement("span");
            meta.className = "fylo-collection-meta";
            const parts = [];
            if (typeof collection.docsStored === "number") parts.push(`${collection.docsStored} docs`);
            if (collection.worm) parts.push("WORM");
            if (collection.error) parts.push("error");
            meta.replaceChildren(...parts.map((part) => {
                const pill = document.createElement("span");
                pill.className = "pill";
                pill.textContent = part;
                return pill;
            }));

            item.append(name, meta);
            item.addEventListener("click", () => selectCollection(collection.name));
            item.addEventListener("keydown", (event) => {
                if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    selectCollection(collection.name);
                }
            });
            collectionsRoot.append(item);
        }
    } catch (error) {
        renderError(collectionsRoot, `Failed to load collections: ${errorMessage(error)}`);
    }
}

/**
 * @param {string} name
 */
async function selectCollection(name) {
    state.selected = name;
    setChipLabel(collectionLabel, name);
    setRestRequest("GET", `${BROWSER_PATH}/${encodeURIComponent(name)}/`, "");
    document.querySelectorAll(".fylo-collection").forEach((node) => {
        const element = /** @type {HTMLElement} */ (node);
        element.setAttribute("aria-selected", element.dataset.collection === name ? "true" : "false");
    });
    if (!documentsRoot) return;
    documentsRoot.replaceChildren(
        Object.assign(document.createElement("p"), { className: "muted md-typescale-body-medium", textContent: "Loading documents…" }),
    );
    try {
        const documentsResponse = await fylo[name].list(25);
        if (documentsResponse.error) {
            renderError(documentsRoot, documentsResponse.error);
            return;
        }
        if (!documentsResponse.docs?.length) {
            documentsRoot.replaceChildren(
                Object.assign(document.createElement("p"), { className: "muted md-typescale-body-medium", textContent: "No documents in this collection." }),
            );
            return;
        }
        const fragments = [];
        const banner = encryptionBanner(documentsResponse.encryptedFields, documentsResponse.revealed);
        if (banner) fragments.push(banner);
        const table = document.createElement("table");
        const thead = document.createElement("thead");
        const headRow = document.createElement("tr");
        for (const label of ["id", "preview"]) {
            const cell = document.createElement("th");
            cell.textContent = label;
            headRow.append(cell);
        }
        thead.append(headRow);
        table.append(thead);
        const tbody = document.createElement("tbody");
        for (const entry of documentsResponse.docs) {
            const row = document.createElement("tr");
            row.className = "fylo-doc-row";
            row.setAttribute("role", "button");
            row.setAttribute("tabindex", "0");
            row.dataset.docId = entry.id;
            const idCell = document.createElement("td");
            idCell.className = "id";
            idCell.textContent = entry.id;
            const previewCell = document.createElement("td");
            previewCell.className = "preview";
            previewCell.textContent = previewOf(entry.doc);
            row.append(idCell, previewCell);
            row.addEventListener("click", () => selectDocument(name, entry.id));
            row.addEventListener("keydown", (event) => {
                if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    selectDocument(name, entry.id);
                }
            });
            tbody.append(row);
        }
        table.append(tbody);
        fragments.push(table);
        documentsRoot.replaceChildren(...fragments);
    } catch (error) {
        renderError(documentsRoot, `Failed to load documents: ${errorMessage(error)}`);
    }
}

/**
 * @param {string[] | undefined} fields
 * @param {boolean | undefined} revealed
 * @returns {HTMLElement | null}
 */
function encryptionBanner(fields, revealed) {
    if (!Array.isArray(fields) || !fields.length) return null;
    const banner = document.createElement("div");
    const prefix = document.createElement("strong");
    prefix.textContent = revealed ? "Encrypted fields shown in plaintext:" : "Encrypted fields masked:";
    const suffix = document.createElement("span");
    suffix.className = "muted";
    suffix.textContent = revealed
        ? "YON_DATA_BROWSER_REVEAL is on — plaintext is sent over the wire."
        : "Set YON_DATA_BROWSER_REVEAL=true to show plaintext.";
    const children = [prefix, document.createTextNode(" ")];
    fields.forEach((field, index) => {
        if (index > 0) children.push(document.createTextNode(", "));
        const code = document.createElement("code");
        code.textContent = field;
        children.push(code);
    });
    children.push(document.createTextNode(". "), suffix);
    if (revealed) {
        banner.className = "fylo-encryption-banner fylo-encryption-banner-revealed";
    } else {
        banner.className = "fylo-encryption-banner";
    }
    banner.replaceChildren(...children);
    return banner;
}

/**
 * @param {unknown} doc
 * @returns {string}
 */
function previewOf(doc) {
    if (!doc || typeof doc !== "object") return String(doc);
    const entries = Object.entries(/** @type {Record<string, unknown>} */ (doc)).slice(0, 4);
    const parts = entries.map(([key, value]) => {
        const rendered = typeof value === "string"
            ? `"${value.length > 40 ? value.slice(0, 40) + "…" : value}"`
            : typeof value === "object" && value !== null
                ? Array.isArray(value) ? `[${value.length}]` : "{…}"
                : String(value);
        return `${key}: ${rendered}`;
    });
    return parts.join(", ");
}

/**
 * @param {string} collection
 * @param {string} id
 */
async function selectDocument(collection, id) {
    state.selectedDocId = id;
    setChipLabel(detailIdLabel, id);
    setRestRequest("GET", `${BROWSER_PATH}/${encodeURIComponent(collection)}/${encodeURIComponent(id)}/`, "");
    document.querySelectorAll(".fylo-doc-row").forEach((node) => {
        const element = /** @type {HTMLElement} */ (node);
        element.setAttribute("aria-selected", element.dataset.docId === id ? "true" : "false");
    });
    if (!detailRoot) return;
    detailRoot.replaceChildren(
        Object.assign(document.createElement("p"), { className: "muted md-typescale-body-medium", textContent: "Loading document…" }),
    );
    try {
        const documentResponse = await fylo[collection].get(id);
        if (documentResponse.error) {
            renderError(detailRoot, documentResponse.error);
            return;
        }
        renderDetail(documentResponse);
    } catch (error) {
        renderError(detailRoot, `Failed to load document: ${errorMessage(error)}`);
    }
}

/**
 * @param {{ doc: Record<string, unknown> | null, docError?: string, encryptedFields?: string[], revealed?: boolean }} documentResponse
 */
function renderDetail(documentResponse) {
    if (!detailRoot) return;
    const fragments = [];

    const banner = encryptionBanner(documentResponse.encryptedFields, documentResponse.revealed);
    if (banner) fragments.push(banner);

    const docHeading = document.createElement("h3");
    docHeading.className = "fylo-detail-heading";
    docHeading.textContent = "Document";
    fragments.push(docHeading);

    if (documentResponse.docError) {
        const errorNode = document.createElement("div");
        errorNode.className = "fylo-error";
        errorNode.textContent = documentResponse.docError;
        fragments.push(errorNode);
    } else if (!documentResponse.doc || Object.keys(documentResponse.doc).length === 0) {
        const note = document.createElement("p");
        note.className = "muted md-typescale-body-medium";
        note.textContent = "No document body returned (may be tombstoned in WORM mode).";
        fragments.push(note);
    } else {
        const bodyPreview = document.createElement("pre");
        bodyPreview.textContent = JSON.stringify(documentResponse.doc, null, 2);
        fragments.push(bodyPreview);
    }

    detailRoot.replaceChildren(...fragments);
}

/**
 * @param {string} method
 * @param {string} path
 * @param {string} body
 */
function setRestRequest(method, path, body) {
    if (requestMethodField) requestMethodField.value = method;
    if (requestPathField) requestPathField.value = path;
    if (queryField) queryField.value = body;
    if (queryModeChip) queryModeChip.textContent = method;
}

function useSelectedRoute() {
    if (state.selected && state.selectedDocId) {
        setRestRequest("GET", `${BROWSER_PATH}/${encodeURIComponent(state.selected)}/${encodeURIComponent(state.selectedDocId)}/`, "");
        return;
    }
    if (state.selected) {
        setRestRequest("GET", `${BROWSER_PATH}/${encodeURIComponent(state.selected)}/`, "");
        return;
    }
    setRestRequest("GET", `${BROWSER_PATH}/`, "");
}

async function runQuery() {
    if (!requestMethodField || !requestPathField || !queryField || !queryResultsRoot) return;
    const method = requestMethodField.value || "GET";
    const requestPath = requestPathField.value || `${BROWSER_PATH}/`;
    const bodySource = queryField.value.trim();
    queryResultsRoot.replaceChildren(
        Object.assign(document.createElement("p"), { className: "muted md-typescale-body-medium", textContent: "Sending request…" }),
    );
    try {
        const headers = new Headers();
        /** @type {BodyInit | undefined} */
        let body;
        if (!["GET", "HEAD", "DELETE"].includes(method) && bodySource) {
            try {
                JSON.parse(bodySource);
            } catch (error) {
                renderError(queryResultsRoot, `Invalid JSON: ${errorMessage(error)}`);
                return;
            }
            headers.set("Content-Type", "application/json");
            body = bodySource;
        }
        const url = requestPath.startsWith("http")
            ? requestPath
            : requestPath.startsWith(BROWSER_PATH)
                ? requestPath
                : `${BROWSER_PATH}${requestPath.startsWith("/") ? "" : "/"}${requestPath}`;
        const response = typeof fylo.request === "function"
            ? await fylo.request(url, { method, headers, body })
            : await fetch(url, { method, headers, body });
        const text = await response.text();
        /** @type {unknown} */
        let payload = text;
        if (text) {
            try {
                payload = JSON.parse(text);
            } catch {
                payload = text;
            }
        }
        renderRestResult({ method, url, status: response.status, statusText: response.statusText, body: payload });
    } catch (error) {
        renderError(queryResultsRoot, `Request failed: ${errorMessage(error)}`);
    }
}

/**
 * @param {{ method: string, url: string, status: number, statusText: string, body: unknown }} result
 */
function renderRestResult(result) {
    if (!queryResultsRoot) return;
    const summary = document.createElement("div");
    summary.className = "fylo-response-summary";
    const method = document.createElement("span");
    method.className = "pill pill-accent";
    method.textContent = result.method;
    const url = document.createElement("code");
    url.textContent = result.url;
    const status = document.createElement("span");
    status.className = "pill";
    status.textContent = `${result.status} ${result.statusText}`;
    summary.append(method, url, status);
    const resultPreview = document.createElement("pre");
    resultPreview.textContent = typeof result.body === "string"
        ? result.body
        : JSON.stringify(result.body, null, 2);
    queryResultsRoot.replaceChildren(summary, resultPreview);
}

if (queryToggleBtn) {
    queryToggleBtn.addEventListener("click", () => {
        useSelectedRoute();
    });
}
if (requestMethodField) {
    requestMethodField.addEventListener("change", () => {
        const method = requestMethodField.value || "GET";
        if (queryModeChip) queryModeChip.textContent = method;
        if (queryField && ["POST", "PUT", "PATCH"].includes(method) && !queryField.value.trim()) {
            queryField.value = JSON.stringify({ title: "Example" }, null, 2);
        }
    });
}
if (queryRunBtn) {
    queryRunBtn.addEventListener("click", () => { runQuery(); });
}

/**
 * @param {string} status
 */
function setEventsStatus(status) {
    if (eventsStatusChip) eventsStatusChip.textContent = status;
}

function stopEventsTail() {
    if (state.eventsTimer) {
        clearInterval(state.eventsTimer);
        state.eventsTimer = null;
    }
    if (eventsToggleBtn) eventsToggleBtn.textContent = "Start tail";
    setEventsStatus("idle");
}

async function pollEvents() {
    if (!state.eventsCollection || !eventsRoot) return;
    try {
        const eventsResponse = await fylo[state.eventsCollection].events(state.eventsOffset);
        if (eventsResponse.error) {
            setEventsStatus("error");
            return;
        }
        if (!eventsResponse.exists) {
            setEventsStatus("no journal");
            return;
        }
        state.eventsOffset = eventsResponse.offset ?? state.eventsOffset;
        if (Array.isArray(eventsResponse.events) && eventsResponse.events.length) {
            appendEvents(eventsResponse.events);
        }
        setEventsStatus(`tailing · ${state.eventsCollection}`);
    } catch (error) {
        setEventsStatus(`error: ${errorMessage(error)}`);
    }
}

/**
 * @param {Array<Record<string, unknown>>} entries
 */
function appendEvents(entries) {
    if (!eventsRoot) return;
    if (eventsRoot.firstElementChild?.classList.contains("muted")) {
        eventsRoot.replaceChildren();
    }
    for (const entry of entries) {
        const item = document.createElement("li");
        item.className = "fylo-event-row";
        const header = document.createElement("div");
        header.className = "fylo-event-head";
        const operation = typeof entry.op === "string" ? entry.op : (typeof entry.kind === "string" ? entry.kind : "event");
        const timestamp = typeof entry.ts === "number"
            ? new Date(entry.ts).toISOString()
            : (typeof entry.timestamp === "number" ? new Date(entry.timestamp).toISOString() : "");
        const operationNode = document.createElement("span");
        operationNode.className = "pill pill-accent";
        operationNode.textContent = operation;
        const timestampNode = document.createElement("span");
        timestampNode.className = "fylo-event-meta";
        timestampNode.textContent = timestamp;
        header.append(operationNode, timestampNode);
        const body = document.createElement("pre");
        body.textContent = JSON.stringify(entry, null, 2);
        item.append(header, body);
        eventsRoot.prepend(item);
    }
    while (eventsRoot.children.length > EVENTS_MAX_ROWS) {
        const last = eventsRoot.lastElementChild;
        if (!last) break;
        eventsRoot.removeChild(last);
    }
}

function startEventsTail() {
    if (!state.selected) {
        setEventsStatus("select a collection first");
        return;
    }
    state.eventsCollection = state.selected;
    state.eventsOffset = 0;
    if (eventsRoot) eventsRoot.replaceChildren();
    if (eventsToggleBtn) eventsToggleBtn.textContent = "Stop tail";
    setEventsStatus(`tailing · ${state.eventsCollection}`);
    pollEvents();
    state.eventsTimer = setInterval(() => { pollEvents(); }, EVENTS_POLL_MS);
}

if (eventsToggleBtn) {
    eventsToggleBtn.addEventListener("click", () => {
        if (state.eventsTimer) {
            stopEventsTail();
        } else {
            startEventsTail();
        }
    });
}

if (eventsClearBtn) {
    eventsClearBtn.addEventListener("click", () => {
        if (eventsRoot) eventsRoot.replaceChildren();
    });
}

loadMeta();
loadCollections();
