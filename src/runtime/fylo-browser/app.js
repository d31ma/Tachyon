// @ts-check

const BROWSER_PATH = "__FYLO_BROWSER_PATH__";

// ── Inline FYLO global bootstrap ───────────────────────────────────────────
// The standalone /_fylo shell doesn't load the app's `imports.js`, so
// `window.fylo` isn't defined. We inline the same bootstrap snippet here so
// every API call can go through the global Proxy instead of raw fetch().
if (!/** @type {any} */ (window).fylo) {
    /**
     * @param {string} path
     * @param {Record<string, unknown>} body
     */
    async function __fyloPostJson(path, body) {
        const r = await fetch(`${BROWSER_PATH}${path}`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(body),
        });
        return r.json();
    }

    /** @param {string} collection */
    function __fyloCollection(collection) {
        return {
            /** @param {Record<string, unknown>} [query] */
            async find(query = {}) {
                return __fyloPostJson("/api/query", { kind: "find", collection, query });
            },
            /** @param {number} [limit] */
            async list(limit = 25) {
                const r = await fetch(`${BROWSER_PATH}/api/docs?collection=${encodeURIComponent(collection)}&limit=${limit}`);
                return r.json();
            },
            /** @param {string} id */
            async get(id) {
                const r = await fetch(`${BROWSER_PATH}/api/doc?collection=${encodeURIComponent(collection)}&id=${encodeURIComponent(id)}`);
                return r.json();
            },
            /** @param {number} [since] */
            async events(since = 0) {
                const r = await fetch(`${BROWSER_PATH}/api/events?collection=${encodeURIComponent(collection)}&since=${since}`);
                return r.json();
            },
            /** @param {string} id @param {Record<string, unknown>} doc */
            async patch(id, doc) {
                return __fyloPostJson("/api/patch", { collection, id, doc });
            },
            /** @param {string} id */
            async del(id) {
                const r = await fetch(`${BROWSER_PATH}/api/delete?collection=${encodeURIComponent(collection)}&id=${encodeURIComponent(id)}`, { method: "DELETE" });
                return r.json();
            },
            async rebuild() {
                return __fyloPostJson("/api/rebuild", { collection });
            },
        };
    }

    const __fyloState = {
        enabled: false,
        /** @type {string | undefined} */
        root: undefined,
        /** @param {string} source */
        sql(source) {
            return __fyloPostJson("/api/query", { kind: "sql", source });
        },
        async collections() {
            const r = await fetch(`${BROWSER_PATH}/api/collections`, { cache: "reload" });
            if (!r.ok) return { root: "", collections: [] };
            const d = await r.json();
            __fyloState.root = d.root;
            return d;
        },
        async meta() {
            const r = await fetch(`${BROWSER_PATH}/api/meta`, { cache: "reload" });
            if (!r.ok) return null;
            return r.json();
        },
    };

    /** @type {any} */ (window).fylo = new Proxy(__fyloState, {
        get(target, prop) {
            if (typeof prop !== "string") return Reflect.get(target, prop);
            if (prop in target) return Reflect.get(target, prop);
            return __fyloCollection(prop);
        },
        set(target, prop, value) {
            return Reflect.set(target, prop, value);
        },
        has(target, prop) {
            return typeof prop === "string" || prop in target;
        },
    });

    // Probe meta once — if the browser route is mounted, flip enabled and cache the root.
    /** @type {any} */ (window).fylo.meta()
        .then((/** @type {any} */ meta) => {
            if (meta) {
                __fyloState.enabled = true;
                __fyloState.root = meta.root;
            }
        })
        .catch(() => { /* fylo browser not mounted — leave disabled */ });
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
    /** @type {"sql" | "find"} */
    queryMode: "sql",
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
        const data = await fylo.collections();
        if (!data.collections.length) {
            collectionsRoot.replaceChildren(
                Object.assign(document.createElement("p"), {
                    className: "muted md-typescale-body-medium",
                    textContent: "No collections found at this root yet.",
                }),
            );
            return;
        }
        collectionsRoot.replaceChildren();
        for (const collection of data.collections) {
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
            meta.innerHTML = parts.map((part) => `<span class="pill">${part}</span>`).join("");

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
    document.querySelectorAll(".fylo-collection").forEach((node) => {
        const element = /** @type {HTMLElement} */ (node);
        element.setAttribute("aria-selected", element.dataset.collection === name ? "true" : "false");
    });
    if (!documentsRoot) return;
    documentsRoot.replaceChildren(
        Object.assign(document.createElement("p"), { className: "muted md-typescale-body-medium", textContent: "Loading documents…" }),
    );
    try {
        const data = await fylo[name].list(25);
        if (data.error) {
            renderError(documentsRoot, data.error);
            return;
        }
        if (!data.docs?.length) {
            documentsRoot.replaceChildren(
                Object.assign(document.createElement("p"), { className: "muted md-typescale-body-medium", textContent: "No documents in this collection." }),
            );
            return;
        }
        const fragments = [];
        const banner = encryptionBanner(data.encryptedFields, data.revealed);
        if (banner) fragments.push(banner);
        const table = document.createElement("table");
        const thead = document.createElement("thead");
        thead.innerHTML = "<tr><th>id</th><th>preview</th></tr>";
        table.append(thead);
        const tbody = document.createElement("tbody");
        for (const entry of data.docs) {
            const tr = document.createElement("tr");
            tr.className = "fylo-doc-row";
            tr.setAttribute("role", "button");
            tr.setAttribute("tabindex", "0");
            tr.dataset.docId = entry.id;
            const idCell = document.createElement("td");
            idCell.className = "id";
            idCell.textContent = entry.id;
            const previewCell = document.createElement("td");
            previewCell.className = "preview";
            previewCell.textContent = previewOf(entry.doc);
            tr.append(idCell, previewCell);
            tr.addEventListener("click", () => selectDocument(name, entry.id));
            tr.addEventListener("keydown", (event) => {
                if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    selectDocument(name, entry.id);
                }
            });
            tbody.append(tr);
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
    const fieldList = fields.map((f) => `<code>${f}</code>`).join(", ");
    if (revealed) {
        banner.className = "fylo-encryption-banner fylo-encryption-banner-revealed";
        banner.innerHTML = `<strong>Encrypted fields shown in plaintext:</strong> ${fieldList}. <span class="muted">YON_DATA_BROWSER_REVEAL is on — plaintext is sent over the wire.</span>`;
    } else {
        banner.className = "fylo-encryption-banner";
        banner.innerHTML = `<strong>Encrypted fields masked:</strong> ${fieldList}. <span class="muted">Set YON_DATA_BROWSER_REVEAL=true to show plaintext.</span>`;
    }
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
    document.querySelectorAll(".fylo-doc-row").forEach((node) => {
        const element = /** @type {HTMLElement} */ (node);
        element.setAttribute("aria-selected", element.dataset.docId === id ? "true" : "false");
    });
    if (!detailRoot) return;
    detailRoot.replaceChildren(
        Object.assign(document.createElement("p"), { className: "muted md-typescale-body-medium", textContent: "Loading document…" }),
    );
    try {
        const data = await fylo[collection].get(id);
        if (data.error) {
            renderError(detailRoot, data.error);
            return;
        }
        renderDetail(data);
    } catch (error) {
        renderError(detailRoot, `Failed to load document: ${errorMessage(error)}`);
    }
}

/**
 * @param {{ doc: Record<string, unknown> | null, history: Array<Record<string, unknown>> | null, docError?: string, historyError?: string, encryptedFields?: string[], revealed?: boolean }} data
 */
function renderDetail(data) {
    if (!detailRoot) return;
    const fragments = [];

    const banner = encryptionBanner(data.encryptedFields, data.revealed);
    if (banner) fragments.push(banner);

    const docHeading = document.createElement("h3");
    docHeading.className = "fylo-detail-heading";
    docHeading.textContent = "Document";
    fragments.push(docHeading);

    if (data.docError) {
        const err = document.createElement("div");
        err.className = "fylo-error";
        err.textContent = data.docError;
        fragments.push(err);
    } else if (!data.doc || Object.keys(data.doc).length === 0) {
        const note = document.createElement("p");
        note.className = "muted md-typescale-body-medium";
        note.textContent = "No document body returned (may be tombstoned in WORM mode).";
        fragments.push(note);
    } else {
        const pre = document.createElement("pre");
        pre.textContent = JSON.stringify(data.doc, null, 2);
        fragments.push(pre);
    }

    const historyHeading = document.createElement("h3");
    historyHeading.className = "fylo-detail-heading";
    historyHeading.textContent = "Version history";
    fragments.push(historyHeading);

    if (data.historyError) {
        const note = document.createElement("p");
        note.className = "muted md-typescale-body-medium";
        note.textContent = `History unavailable: ${data.historyError}`;
        fragments.push(note);
    } else if (!data.history?.length) {
        const note = document.createElement("p");
        note.className = "muted md-typescale-body-medium";
        note.textContent = "No version history (collection is not WORM, or single retained version).";
        fragments.push(note);
    } else {
        const list = document.createElement("ol");
        list.className = "fylo-history";
        for (const entry of data.history) {
            const item = document.createElement("li");
            item.className = "fylo-history-entry";
            const head = document.createElement("div");
            head.className = "fylo-history-head";
            const tags = [];
            if (entry.isHead) tags.push('<span class="pill pill-accent">HEAD</span>');
            if (entry.deleted) tags.push('<span class="pill pill-danger">deleted</span>');
            head.innerHTML = `<code>${entry.id}</code>${tags.length ? " " + tags.join(" ") : ""}`;
            const meta = document.createElement("div");
            meta.className = "fylo-history-meta";
            const created = typeof entry.createdAt === "number" ? new Date(entry.createdAt).toISOString() : String(entry.createdAt ?? "");
            const updated = typeof entry.updatedAt === "number" ? new Date(entry.updatedAt).toISOString() : String(entry.updatedAt ?? "");
            meta.textContent = `created ${created} · updated ${updated}`;
            const body = document.createElement("pre");
            body.textContent = JSON.stringify(entry.data, null, 2);
            item.append(head, meta, body);
            list.append(item);
        }
        fragments.push(list);
    }

    detailRoot.replaceChildren(...fragments);
}

/** @param {"sql" | "find"} mode */
function setQueryMode(mode) {
    state.queryMode = mode;
    if (queryModeChip) queryModeChip.textContent = mode === "sql" ? "SQL" : "findDocs";
    if (queryToggleBtn) queryToggleBtn.textContent = mode === "sql" ? "Switch to findDocs" : "Switch to SQL";
    if (queryField) {
        const labelAttr = mode === "sql" ? "SQL statement" : "JSON request — { collection, query }";
        queryField.setAttribute("aria-label", labelAttr);
        queryField.setAttribute("placeholder", labelAttr);
        const sample = mode === "sql"
            ? "SELECT * FROM otel-spans"
            : JSON.stringify({ collection: "otel-spans", query: { $ops: [] } }, null, 2);
        queryField.value = sample;
    }
}

async function runQuery() {
    if (!queryField || !queryResultsRoot) return;
    const source = queryField.value ?? "";
    queryResultsRoot.replaceChildren(
        Object.assign(document.createElement("p"), { className: "muted md-typescale-body-medium", textContent: "Running query…" }),
    );
    try {
        /** @type {Record<string, unknown>} */
        let data;
        if (state.queryMode === "sql") {
            data = await fylo.sql(source);
        } else {
            /** @type {{ collection?: string, query?: Record<string, unknown> }} */
            let parsed;
            try {
                parsed = JSON.parse(source);
            } catch (error) {
                renderError(queryResultsRoot, `Invalid JSON: ${errorMessage(error)}`);
                return;
            }
            const collection = (parsed.collection ?? "").toString();
            if (!collection) {
                renderError(queryResultsRoot, "collection is required for find queries");
                return;
            }
            data = await fylo[collection].find(parsed.query ?? {});
        }
        if (data.error) {
            renderError(queryResultsRoot, /** @type {string} */ (data.error));
            return;
        }
        renderQueryResult(data);
    } catch (error) {
        renderError(queryResultsRoot, `Request failed: ${errorMessage(error)}`);
    }
}

/** @param {Record<string, unknown> & { kind?: string, docs?: Array<{ id: string, doc: unknown }>, result?: unknown }} data */
function renderQueryResult(data) {
    if (!queryResultsRoot) return;
    if (data.kind === "find" && Array.isArray(data.docs)) {
        if (!data.docs.length) {
            queryResultsRoot.replaceChildren(
                Object.assign(document.createElement("p"), { className: "muted md-typescale-body-medium", textContent: "Query returned no documents." }),
            );
            return;
        }
        const table = document.createElement("table");
        const thead = document.createElement("thead");
        thead.innerHTML = "<tr><th>id</th><th>preview</th></tr>";
        table.append(thead);
        const tbody = document.createElement("tbody");
        for (const entry of data.docs) {
            const tr = document.createElement("tr");
            const idCell = document.createElement("td");
            idCell.className = "id";
            idCell.textContent = entry.id;
            const previewCell = document.createElement("td");
            previewCell.className = "preview";
            previewCell.textContent = previewOf(entry.doc);
            tr.append(idCell, previewCell);
            tbody.append(tr);
        }
        table.append(tbody);
        queryResultsRoot.replaceChildren(table);
        return;
    }
    if (data.kind === "sql") {
        const pre = document.createElement("pre");
        pre.textContent = JSON.stringify(data.result, null, 2);
        queryResultsRoot.replaceChildren(pre);
        return;
    }
    const pre = document.createElement("pre");
    pre.textContent = JSON.stringify(data, null, 2);
    queryResultsRoot.replaceChildren(pre);
}

if (queryToggleBtn) {
    queryToggleBtn.addEventListener("click", () => {
        setQueryMode(state.queryMode === "sql" ? "find" : "sql");
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
        const data = await fylo[state.eventsCollection].events(state.eventsOffset);
        if (data.error) {
            setEventsStatus("error");
            return;
        }
        if (!data.exists) {
            setEventsStatus("no journal");
            return;
        }
        state.eventsOffset = data.offset ?? state.eventsOffset;
        if (Array.isArray(data.events) && data.events.length) {
            appendEvents(data.events);
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
        const li = document.createElement("li");
        li.className = "fylo-event-row";
        const head = document.createElement("div");
        head.className = "fylo-event-head";
        const op = typeof entry.op === "string" ? entry.op : (typeof entry.kind === "string" ? entry.kind : "event");
        const ts = typeof entry.ts === "number"
            ? new Date(entry.ts).toISOString()
            : (typeof entry.timestamp === "number" ? new Date(entry.timestamp).toISOString() : "");
        head.innerHTML = `<span class="pill pill-accent">${op}</span> <span class="fylo-event-meta">${ts}</span>`;
        const body = document.createElement("pre");
        body.textContent = JSON.stringify(entry, null, 2);
        li.append(head, body);
        eventsRoot.prepend(li);
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
