// @ts-check
import Tac from "../tac.js";
import { onMount } from "../decorators.js";
import { fylo } from "../fylo-global.js";

type Collection = {
    name: string;
    docsStored?: number;
    indexedDocs?: number;
    worm?: boolean;
    error?: string;
};

type DocEntry = {
    id: string;
    doc: Record<string, unknown>;
};

type HistoryEntry = {
    id: string;
    data: Record<string, unknown>;
    createdAt?: number;
    updatedAt?: number;
    isHead?: boolean;
    deleted?: boolean;
};

type QueryResult = {
    kind?: string;
    docs?: DocEntry[];
    result?: unknown;
    error?: string;
};

type EventEntry = {
    op?: string;
    kind?: string;
    ts?: number;
    timestamp?: number;
    [key: string]: unknown;
};

export default class FyloBrowser extends Tac {
    static tagName = "fylo-browser";

    /** @type {string | null} */
    rootLabel = null;
    readOnly = false;
    enabled = false;

    /** @type {Collection[]} */
    collections = [];
    /** @type {string | null} */
    selectedCollection = null;
    /** @type {DocEntry[]} */
    documents = [];
    /** @type {string | null} */
    selectedDocId = null;
    /** @type {Record<string, unknown> | null} */
    detailDoc = null;
    /** @type {HistoryEntry[]} */
    detailHistory = [];
    /** @type {string | null} */
    detailError = null;
    /** @type {string | null} */
    historyError = null;
    /** @type {string[] | undefined} */
    encryptedFields = undefined;
    /** @type {boolean | undefined} */
    revealed = undefined;

    /** @type {"sql" | "find"} */
    queryMode = "sql";
    querySource = "";
    /** @type {QueryResult | null} */
    queryResult = null;
    /** @type {string | null} */
    queryError = null;

    /** @type {EventEntry[]} */
    events = [];
    eventsStatus = "idle";
    eventsCollection: string | null = null;
    eventsOffset = 0;
    /** @type {ReturnType<typeof setInterval> | null} */
    eventsTimer = null;
    /** @type {boolean} */
    sessionRevealed = false;
    /** @type {string | undefined} */
    revealKey = undefined;

    collectionsError: string | null = null;
    documentsError: string | null = null;
    detailLoading = false;
    queryLoading = false;

    @onMount
    async init(): Promise<void> {
        this.enabled = this.env("YON_DATA_BROWSER_ENABLED", true);
        this.readOnly = this.env("YON_DATA_BROWSER_READONLY", false);
        this.revealed = this.env("YON_DATA_BROWSER_REVEAL", false);

        if (!this.enabled) return;

        try {
            const meta = await fylo.meta();
            if (meta) {
                this.rootLabel = `${meta.root}${meta.readOnly ? " · read-only" : ""}`;
            } else {
                this.rootLabel = "root: unknown";
            }
        } catch {
            this.rootLabel = "root: unknown";
        }

        await this.loadCollections();
    }

    async loadCollections(): Promise<void> {
        this.collectionsError = null;
        try {
            const data = await fylo.collections();
            this.collections = data.collections || [];
        } catch (error) {
            this.collectionsError = `Failed to load collections: ${error instanceof Error ? error.message : String(error)}`;
        }
    }

    async selectCollection(name: string): Promise<void> {
        this.selectedCollection = name;
        this.selectedDocId = null;
        this.documents = [];
        this.documentsError = null;
        this.detailDoc = null;
        this.detailHistory = [];
        this.detailError = null;
        this.historyError = null;
        this.encryptedFields = undefined;

        try {
            const data = await fylo[name].list(25);
            if (data.error) {
                this.documentsError = data.error;
                return;
            }
            this.documents = data.docs || [];
            this.encryptedFields = data.encryptedFields;
            this.revealed = data.revealed;
        } catch (error) {
            this.documentsError = `Failed to load documents: ${error instanceof Error ? error.message : String(error)}`;
        }
    }

    async selectDocument(collection: string, id: string): Promise<void> {
        this.selectedDocId = id;
        this.detailLoading = true;
        this.detailDoc = null;
        this.detailHistory = [];
        this.detailError = null;
        this.historyError = null;
        this.encryptedFields = undefined;

        try {
            const data = await fylo[collection].get(id);
            if (data.error) {
                this.detailError = data.error;
                this.detailLoading = false;
                return;
            }
            this.detailDoc = data.doc;
            this.detailHistory = data.history || [];
            this.historyError = data.historyError || null;
            this.encryptedFields = data.encryptedFields;
            this.revealed = data.revealed;
        } catch (error) {
            this.detailError = `Failed to load document: ${error instanceof Error ? error.message : String(error)}`;
        } finally {
            this.detailLoading = false;
        }
    }

    toggleQueryMode(): void {
        this.queryMode = this.queryMode === "sql" ? "find" : "sql";
        if (this.queryMode === "sql") {
            this.querySource = "SELECT * FROM otel-spans";
        } else {
            this.querySource = JSON.stringify({ collection: "otel-spans", query: { $ops: [] } }, null, 2);
        }
    }

    async runQuery(): Promise<void> {
        this.queryLoading = true;
        this.queryResult = null;
        this.queryError = null;

        try {
            /** @type {Record<string, unknown>} */
            let data;
            if (this.queryMode === "sql") {
                data = await fylo.sql(this.querySource);
            } else {
                /** @type {{ collection?: string, query?: Record<string, unknown> }} */
                let parsed;
                try {
                    parsed = JSON.parse(this.querySource);
                } catch (error) {
                    this.queryError = `Invalid JSON: ${error instanceof Error ? error.message : String(error)}`;
                    this.queryLoading = false;
                    return;
                }
                const collection = (parsed.collection ?? "").toString();
                if (!collection) {
                    this.queryError = "collection is required for find queries";
                    this.queryLoading = false;
                    return;
                }
                data = await fylo[collection].find(parsed.query ?? {});
            }
            if (data.error) {
                this.queryError = data.error;
                return;
            }
            this.queryResult = data;
        } catch (error) {
            this.queryError = `Request failed: ${error instanceof Error ? error.message : String(error)}`;
        } finally {
            this.queryLoading = false;
        }
    }

    startEventsTail(): void {
        if (!this.selectedCollection) {
            this.eventsStatus = "select a collection first";
            return;
        }
        this.eventsCollection = this.selectedCollection;
        this.eventsOffset = 0;
        this.events = [];
        this.eventsStatus = `tailing · ${this.eventsCollection}`;
        this.pollEvents();
        this.eventsTimer = setInterval(() => this.pollEvents(), 3000);
    }

    stopEventsTail(): void {
        if (this.eventsTimer) {
            clearInterval(this.eventsTimer);
            this.eventsTimer = null;
        }
        this.eventsStatus = "idle";
    }

    async pollEvents(): Promise<void> {
        if (!this.eventsCollection) return;

        try {
            const data = await fylo[this.eventsCollection].events(this.eventsOffset);
            if (data.error) {
                this.eventsStatus = "error";
                return;
            }
            if (!data.exists) {
                this.eventsStatus = "no journal";
                return;
            }
            this.eventsOffset = data.offset ?? this.eventsOffset;
            if (Array.isArray(data.events) && data.events.length) {
                this.events = [...data.events.reverse(), ...this.events].slice(0, 200);
            }
            this.eventsStatus = `tailing · ${this.eventsCollection}`;
        } catch (error) {
            this.eventsStatus = `error: ${error instanceof Error ? error.message : String(error)}`;
        }
    }

    clearEvents(): void {
        this.events = [];
    }

    toggleEventsTail(): void {
        if (this.eventsTimer) {
            this.stopEventsTail();
        } else {
            this.startEventsTail();
        }
    }

    previewOf(doc: unknown): string {
        if (!doc || typeof doc !== "object") return String(doc);
        const entries = Object.entries(/** @type {Record<string, unknown>} */ (doc)).slice(0, 4);
        const parts = entries.map(([key, value]) => {
            const rendered =
                typeof value === "string"
                    ? `"${value.length > 40 ? value.slice(0, 40) + "…" : value}"`
                    : typeof value === "object" && value !== null
                        ? Array.isArray(value)
                            ? `[${value.length}]`
                            : "{…}"
                        : String(value);
            return `${key}: ${rendered}`;
        });
        return parts.join(", ");
    }

    formatTs(ts: number): string {
        return new Date(ts).toISOString();
    }

    encryptionBannerText(): string {
        if (!this.encryptedFields?.length) return "";
        const fieldList = this.encryptedFields.map((f) => `<code>${f}</code>`).join(", ");
        if (this.revealed) {
            return `<strong>Encrypted fields shown in plaintext:</strong> ${fieldList}. <span class="muted">YON_DATA_BROWSER_REVEAL is on — plaintext is sent over the wire.</span>`;
        }
        if (this.sessionRevealed) {
            return `<strong>Encrypted fields decrypted for this session:</strong> ${fieldList}. <span class="muted">Reveal key verified — fields are visible.</span>`;
        }
        return `<strong>Encrypted fields masked:</strong> ${fieldList}. <span class="muted">Enter reveal key to decrypt for this session.</span>`;
    }

    async revealFields(key: string): Promise<void> {
        const browserPath = this.env("YON_DATA_BROWSER_PATH", "/_fylo");
        try {
            const res = await this.tac.fetch(`${browserPath}/api/reveal`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ key }),
            });
            const data = await res.json();
            if (data.ok && data.revealed) {
                this.sessionRevealed = true;
            }
        } catch {
            // Silently fail — the UI will stay masked
        }
    }

    render(): string {
        if (!this.enabled) {
            return `<div class="fylo-error">Data Browser is not enabled. Set YON_DATA_BROWSER_ENABLED=true to use it.</div>`;
        }

        const rootLabelText = this.rootLabel ?? "loading…";
        const collectionLabelText = this.selectedCollection ?? "—";
        const queryModeLabel = this.queryMode === "sql" ? "SQL" : "findDocs";
        const queryToggleLabel = this.queryMode === "sql" ? "Switch to findDocs" : "Switch to SQL";
        const queryFieldLabel = this.queryMode === "sql" ? "SQL statement" : "JSON request — { collection, query }";

        return `<div class="fylo-shell">
    <div class="fylo-hero">
        <p class="eyebrow md-typescale-label-large">Fylo Browser</p>
        <h1><code>/_fylo</code></h1>
        <p class="lede">Native database browser for any collection stored by <code>@d31ma/fylo</code>.</p>
        <span class="chip">${rootLabelText}</span>
    </div>

    <div class="fylo-panel">
        <div class="fylo-panel-header">
            <h2 class="md-typescale-title-large">Collections</h2>
        </div>
        <div class="fylo-collections">
            ${this.collectionsError
            ? `<div class="fylo-error">${this.collectionsError}</div>`
            : this.collections.length === 0
                ? `<p class="muted md-typescale-body-medium">No collections found at this root yet.</p>`
                : this.collections.map((c) => `
                <div class="fylo-collection"
                     aria-selected="${this.selectedCollection === c.name ? "true" : "false"}"
                     role="button" tabindex="0"
                     @click="selectCollection('${c.name}')">
                    <span class="fylo-collection-name md-typescale-title-medium">${c.name}</span>
                    <span class="fylo-collection-meta">
                        ${typeof c.docsStored === "number" ? `<span class="pill">${c.docsStored} docs</span>` : ""}
                        ${c.worm ? `<span class="pill">WORM</span>` : ""}
                        ${c.error ? `<span class="pill">error</span>` : ""}
                    </span>
                </div>`).join("")}
        </div>
    </div>

    <div class="fylo-panel">
        <div class="fylo-panel-header">
            <h2 class="md-typescale-title-large">Documents</h2>
            <span class="chip">${collectionLabelText}</span>
        </div>
        <div class="fylo-documents">
            ${this.documentsError
            ? `<div class="fylo-error">${this.documentsError}</div>`
            : this.selectedCollection === null
                ? `<p class="muted md-typescale-body-medium">Select a collection to browse documents.</p>`
                : this.documents.length === 0
                    ? `<p class="muted md-typescale-body-medium">No documents in this collection.</p>`
                    : `<table>
                        <thead><tr><th>id</th><th>preview</th></tr></thead>
                        <tbody>
                            ${this.documents.map((d) => `
                            <tr class="fylo-doc-row"
                                aria-selected="${this.selectedDocId === d.id ? "true" : "false"}"
                                role="button" tabindex="0"
                                @click="selectDocument('${this.selectedCollection}', '${d.id}')">
                                <td class="id">${d.id}</td>
                                <td class="preview">${this.previewOf(d.doc)}</td>
                            </tr>`).join("")}
                        </tbody>
                    </table>`}
        </div>
    </div>

    <div class="fylo-panel">
        <div class="fylo-panel-header">
            <h2 class="md-typescale-title-large">Document Detail</h2>
            <span class="chip">${this.selectedDocId ?? "none selected"}</span>
        </div>
        <div class="fylo-detail">
            ${this.detailLoading
            ? `<p class="muted md-typescale-body-medium">Loading document…</p>`
            : this.detailError
                ? `<div class="fylo-error">${this.detailError}</div>`
                : this.encryptionBannerText()
                    ? `<div class="fylo-encryption-banner ${this.revealed || this.sessionRevealed ? "fylo-encryption-banner-revealed" : ""}">${this.encryptionBannerText()}${!this.revealed && !this.sessionRevealed ? `<div class="fylo-reveal-ui"><input class="field-control" type="password" placeholder="Reveal key" @input="revealKey = event.target.value" value="${this.revealKey ?? ""}" /> <button type="button" class="button button-primary" @click="revealFields(revealKey)">Reveal</button></div>` : ""}</div>`
                    : ""}
            ${this.selectedDocId !== null && !this.detailLoading ? `
                <h3 class="fylo-detail-heading">Document</h3>
                ${!this.detailDoc || Object.keys(this.detailDoc).length === 0
                    ? `<p class="muted md-typescale-body-medium">No document body returned (may be tombstoned in WORM mode).</p>`
                    : `<pre>${JSON.stringify(this.detailDoc, null, 2)}</pre>`}

                <h3 class="fylo-detail-heading">Version history</h3>
                ${this.historyError
                    ? `<p class="muted md-typescale-body-medium">History unavailable: ${this.historyError}</p>`
                    : this.detailHistory.length === 0
                        ? `<p class="muted md-typescale-body-medium">No version history (collection is not WORM, or single retained version).</p>`
                        : `<ol class="fylo-history">
                            ${this.detailHistory.map((entry) => `
                            <li class="fylo-history-entry">
                                <div class="fylo-history-head">
                                    <code>${entry.id}</code>
                                    ${entry.isHead ? `<span class="pill pill-accent">HEAD</span>` : ""}
                                    ${entry.deleted ? `<span class="pill pill-danger">deleted</span>` : ""}
                                </div>
                                <div class="fylo-history-meta">
                                    created ${typeof entry.createdAt === "number" ? this.formatTs(entry.createdAt) : String(entry.createdAt ?? "")} ·
                                    updated ${typeof entry.updatedAt === "number" ? this.formatTs(entry.updatedAt) : String(entry.updatedAt ?? "")}
                                </div>
                                <pre>${JSON.stringify(entry.data, null, 2)}</pre>
                            </li>`).join("")}
                        </ol>`}
            ` : ""}
        </div>
    </div>

    <div class="fylo-panel">
        <div class="fylo-panel-header">
            <h2 class="md-typescale-title-large">Query</h2>
            <span class="chip">${queryModeLabel}</span>
        </div>
        <div class="fylo-query">
            <textarea
                class="field-control"
                rows="5"
                aria-label="${queryFieldLabel}"
                placeholder="${queryFieldLabel}"
                @input="querySource = event.target.value"
            >${this.querySource || (this.queryMode === "sql" ? "SELECT * FROM otel-spans" : `{ "collection": "otel-spans", "query": { "$ops": [] } }`)}</textarea>
            <div class="fylo-query-actions">
                <button type="button" class="button button-text" @click="toggleQueryMode">${queryToggleLabel}</button>
                <button type="button" class="button button-primary" @click="runQuery">Run</button>
            </div>
        </div>
        <div class="fylo-query-results">
            ${this.queryLoading
            ? `<p class="muted md-typescale-body-medium">Running query…</p>`
            : this.queryError
                ? `<div class="fylo-error">${this.queryError}</div>`
                : this.queryResult
                    ? this.renderQueryResult(this.queryResult)
                    : `<p class="muted md-typescale-body-medium">Run a SQL statement or findDocs query.</p>`}
        </div>
    </div>

    <div class="fylo-panel">
        <div class="fylo-panel-header">
            <h2 class="md-typescale-title-large">Events</h2>
            <span class="chip">${this.eventsStatus}</span>
        </div>
        <div class="fylo-events-actions">
            <button type="button" class="button button-text" @click="toggleEventsTail">${this.eventsTimer ? "Stop tail" : "Start tail"}</button>
            <button type="button" class="button button-text" @click="clearEvents">Clear</button>
        </div>
        <ul class="fylo-events">
            ${this.events.length === 0
            ? `<p class="muted md-typescale-body-medium">No events yet. Select a collection and start tailing.</p>`
            : this.events.map((entry) => `
                <li class="fylo-event-row">
                    <div class="fylo-event-head">
                        <span class="pill pill-accent">${entry.op || entry.kind || "event"}</span>
                        <span class="fylo-event-meta">${typeof entry.ts === "number" ? this.formatTs(entry.ts) : typeof entry.timestamp === "number" ? this.formatTs(entry.timestamp) : ""}</span>
                    </div>
                    <pre>${JSON.stringify(entry, null, 2)}</pre>
                </li>`).join("")}
        </ul>
    </div>
</div>`;
    }

    renderQueryResult(data: QueryResult): string {
        if (data.kind === "find" && Array.isArray(data.docs)) {
            if (!data.docs.length) {
                return `<p class="muted md-typescale-body-medium">Query returned no documents.</p>`;
            }
            return `<table>
                <thead><tr><th>id</th><th>preview</th></tr></thead>
                <tbody>
                    ${data.docs.map((entry) => `
                    <tr>
                        <td class="id">${entry.id}</td>
                        <td class="preview">${this.previewOf(entry.doc)}</td>
                    </tr>`).join("")}
                </tbody>
            </table>`;
        }
        if (data.kind === "sql") {
            return `<pre>${JSON.stringify(data.result, null, 2)}</pre>`;
        }
        return `<pre>${JSON.stringify(data, null, 2)}</pre>`;
    }
}
