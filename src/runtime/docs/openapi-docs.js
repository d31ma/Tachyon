// @ts-check

const OPENAPI_PATH = "__OPENAPI_PATH__";
const DOCS_PATH = "__OPENAPI_DOCS_PATH__";
const STORAGE_KEY = "tachyon:api-docs:stable";
const methodOrder = ["get", "post", "put", "patch", "delete", "head", "options"];

/** @type {HTMLElement | null} */
const docsRoot = document.querySelector("#docs-app");
/** @type {HTMLElement | null} */
const summaryRoot = document.querySelector("#docs-summary");
/** @type {HTMLElement | null} */
const navRoot = document.querySelector("#docs-nav");
/** @type {HTMLElement | null} */
const routesRoot = document.querySelector("#docs-routes");
/** @type {HTMLElement | null} */
const originRoot = document.querySelector("#docs-origin");
/** @type {HTMLInputElement | null} */
const filterInput = document.querySelector("#docs-filter");
/** @type {HTMLSelectElement | null} */
const serverSelect = document.querySelector("#docs-server");
/** @type {HTMLElement | null} */
const authRoot = document.querySelector("#docs-auth");

/**
 * @typedef {{
 *   expanded: boolean,
 *   tryItOut: boolean,
 *   loading: boolean,
 *   values: {
 *     path: Record<string, string>,
 *     query: Record<string, string>,
 *     header: Record<string, string>,
 *     body: string,
 *     contentType: string,
 *   },
 *   response: null | {
 *     ok: boolean,
 *     status: number,
 *     statusText: string,
 *     durationMs: number,
 *     contentType: string,
 *     headers: Array<[string, string]>,
 *     bodyText: string,
 *   }
 * }} OperationState
 */

/** @type {{
 *   spec: any,
 *   filter: string,
 *   server: string,
 *   auth: Record<string, Record<string, string>>,
 *   operations: Record<string, OperationState>,
 * }} */
const state = {
  spec: null,
  filter: "",
  server: "",
  auth: {},
  operations: {},
};

/** @param {unknown} value */
function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/** @param {unknown} value */
function prettyJson(value) {
  return escapeHtml(JSON.stringify(value, null, 2));
}

/** @param {string} value */
function titleCase(value) {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

/** @param {string} pathname @param {string} method */
function operationSlug(pathname, method) {
  return `operation-${method}-${pathname}`
    .toLowerCase()
    .replaceAll("{", "")
    .replaceAll("}", "")
    .replaceAll("/", "-")
    .replaceAll(/[^a-z0-9-]/g, "-")
    .replaceAll(/-+/g, "-")
    .replace(/^-|-$/g, "");
}

function hasSessionStorage() {
  try {
    sessionStorage.setItem("__tachyon_docs__", "1");
    sessionStorage.removeItem("__tachyon_docs__");
    return true;
  } catch {
    return false;
  }
}

function loadPersistedState() {
  if (!hasSessionStorage()) return;
  try {
    const stored = sessionStorage.getItem(STORAGE_KEY);
    if (!stored) return;
    const parsed = JSON.parse(stored);
    if (parsed && typeof parsed === "object") {
      if (typeof parsed.filter === "string") state.filter = parsed.filter;
      if (typeof parsed.server === "string") state.server = parsed.server;
      if (parsed.auth && typeof parsed.auth === "object") state.auth = parsed.auth;
    }
  } catch {
    // Ignore corrupt state and let the docs rebuild from scratch.
  }
}

function persistState() {
  if (!hasSessionStorage()) return;
  try {
    sessionStorage.setItem(STORAGE_KEY, JSON.stringify({
      filter: state.filter,
      server: state.server,
      auth: state.auth,
    }));
  } catch {
    // Ignore storage failures.
  }
}

/**
 * @param {any} schema
 * @param {WeakSet<object>} [seen]
 * @returns {unknown}
 */
function exampleFromSchema(schema, seen = new WeakSet()) {
  if (!schema || typeof schema !== "object") return "";
  if (Object.hasOwn(schema, "example")) return schema.example;
  if (Array.isArray(schema.enum) && schema.enum.length > 0) return schema.enum[0];
  if (Array.isArray(schema.oneOf) && schema.oneOf.length > 0) return exampleFromSchema(schema.oneOf[0], seen);
  if (Array.isArray(schema.anyOf) && schema.anyOf.length > 0) return exampleFromSchema(schema.anyOf[0], seen);
  if (schema.type === "array") return [exampleFromSchema(schema.items || {}, seen)];
  if (schema.type === "object" || schema.properties || schema.additionalProperties) {
    if (seen.has(schema)) return {};
    seen.add(schema);
    /** @type {Record<string, unknown>} */
    const value = {};
    for (const [key, propertySchema] of Object.entries(schema.properties || {})) {
      value[key] = exampleFromSchema(propertySchema, seen);
    }
    if (Object.keys(value).length === 0 && schema.additionalProperties) {
      value.sample = exampleFromSchema(schema.additionalProperties, seen);
    }
    return value;
  }
  if (schema.type === "integer" || schema.type === "number") return 0;
  if (schema.type === "boolean") return false;
  if (schema.type === "null") return null;
  if (schema.format === "date-time") return new Date().toISOString();
  return "";
}

/**
 * @param {any} schema
 * @returns {string}
 */
function schemaLabel(schema) {
  if (!schema || typeof schema !== "object") return "any";
  if (Array.isArray(schema.enum) && schema.enum.length > 0) return `enum(${schema.enum.join(", ")})`;
  if (schema.type) return String(schema.type);
  if (schema.oneOf) return "oneOf";
  if (schema.anyOf) return "anyOf";
  return "any";
}

/**
 * @param {any} param
 * @returns {string}
 */
function defaultParameterValue(param) {
  if (Object.hasOwn(param, "example")) return String(param.example ?? "");
  const example = exampleFromSchema(param.schema || {});
  if (example === null || example === undefined) return "";
  if (typeof example === "object") return JSON.stringify(example);
  return String(example);
}

/**
 * @param {any} operation
 * @returns {{ contentType: string, body: string }}
 */
function defaultRequestBody(operation) {
  const content = operation.requestBody?.content || {};
  const entries = Object.entries(content);
  if (entries.length === 0) {
    return { contentType: "application/json", body: "" };
  }
  const [contentType, descriptor] = entries[0];
  const example = exampleFromSchema(descriptor?.schema || {});
  const body = typeof example === "string"
    ? example
    : JSON.stringify(example, null, 2);
  return {
    contentType,
    body: body === "undefined" ? "" : body,
  };
}

/**
 * @param {string} operationId
 * @param {any} operation
 * @returns {OperationState}
 */
function ensureOperationState(operationId, operation) {
  if (state.operations[operationId]) return state.operations[operationId];
  /** @type {OperationState["values"]["path"]} */
  const pathValues = {};
  /** @type {OperationState["values"]["query"]} */
  const queryValues = {};
  /** @type {OperationState["values"]["header"]} */
  const headerValues = {};
  for (const parameter of operation.parameters || []) {
    const target = parameter.in === "path"
      ? pathValues
      : parameter.in === "query"
        ? queryValues
        : headerValues;
    target[parameter.name] = defaultParameterValue(parameter);
  }
  const bodyDefaults = defaultRequestBody(operation);
  state.operations[operationId] = {
    expanded: window.location.hash === `#${operationId}`,
    tryItOut: false,
    loading: false,
    values: {
      path: pathValues,
      query: queryValues,
      header: headerValues,
      body: bodyDefaults.body,
      contentType: bodyDefaults.contentType,
    },
    response: null,
  };
  return state.operations[operationId];
}

/**
 * @param {any} spec
 * @returns {string[]}
 */
function securitySchemeNames(spec) {
  return Object.keys(spec?.components?.securitySchemes || {});
}

/**
 * @param {Record<string, any>} schemeMap
 * @returns {string}
 */
function renderAuthFields(schemeMap) {
  const names = Object.keys(schemeMap);
  if (names.length === 0) {
    return "<p class=\"muted\">No security schemes are declared in this OpenAPI document.</p>";
  }
  return names.map((name) => {
    const scheme = schemeMap[name];
    const values = state.auth[name] || {};
    if (scheme.type === "http" && scheme.scheme === "basic") {
      return `
        <article class="auth-card">
          <div class="auth-card-header">
            <strong>${escapeHtml(name)}</strong>
            <span class="meta-chip">basic auth</span>
          </div>
          <label class="field">
            <span>Username</span>
            <input type="text" data-auth-scheme="${escapeHtml(name)}" data-auth-field="username" value="${escapeHtml(values.username || "")}" autocomplete="username">
          </label>
          <label class="field">
            <span>Password</span>
            <input type="password" data-auth-scheme="${escapeHtml(name)}" data-auth-field="password" value="${escapeHtml(values.password || "")}" autocomplete="current-password">
          </label>
        </article>
      `;
    }
    if (scheme.type === "http" && scheme.scheme === "bearer") {
      return `
        <article class="auth-card">
          <div class="auth-card-header">
            <strong>${escapeHtml(name)}</strong>
            <span class="meta-chip">bearer token</span>
          </div>
          <label class="field">
            <span>Token</span>
            <input type="password" data-auth-scheme="${escapeHtml(name)}" data-auth-field="token" value="${escapeHtml(values.token || "")}" autocomplete="off">
          </label>
        </article>
      `;
    }
    if (scheme.type === "apiKey") {
      return `
        <article class="auth-card">
          <div class="auth-card-header">
            <strong>${escapeHtml(name)}</strong>
            <span class="meta-chip">api key in ${escapeHtml(scheme.in || "header")}</span>
          </div>
          <label class="field">
            <span>${escapeHtml(scheme.name || "API key")}</span>
            <input type="password" data-auth-scheme="${escapeHtml(name)}" data-auth-field="value" value="${escapeHtml(values.value || "")}" autocomplete="off">
          </label>
        </article>
      `;
    }
    return `
      <article class="auth-card">
        <div class="auth-card-header">
          <strong>${escapeHtml(name)}</strong>
          <span class="meta-chip">${escapeHtml(scheme.type || "security")}</span>
        </div>
        <p class="muted">This docs console cannot automate ${escapeHtml(scheme.type || "this")} flows yet, but the scheme is still exposed in the spec.</p>
      </article>
    `;
  }).join("");
}

/** @param {any} spec */
function renderSummary(spec) {
  if (!summaryRoot) return;
  const paths = Object.entries(spec.paths || {});
  const operationCount = paths.reduce((total, [, operations]) => total + Object.keys(operations).length, 0);
  const tags = new Set();
  for (const [, operations] of paths) {
    for (const operation of Object.values(operations)) {
      for (const tag of operation.tags || []) tags.add(tag);
    }
  }
  summaryRoot.innerHTML = [
    ["Version", escapeHtml(spec.info?.version || "n/a")],
    ["Routes", String(paths.length)],
    ["Operations", String(operationCount)],
    ["Tags", String(tags.size)],
  ].map(([label, value]) => `
    <article class="summary-card">
      <span class="summary-label">${label}</span>
      <strong>${value}</strong>
    </article>
  `).join("");
}

/**
 * @param {[string, Record<string, any>][]} routeEntries
 */
function renderNav(routeEntries) {
  if (!navRoot) return;
  navRoot.innerHTML = routeEntries.map(([pathname, operations]) => {
    const operationLinks = methodOrder
      .filter((method) => operations[method])
      .map((method) => {
        const operationId = operationSlug(pathname, method);
        return `<a href="#${operationId}" class="nav-operation-link"><span class="method-pill method-${method}">${escapeHtml(method)}</span><span>${escapeHtml(pathname)}</span></a>`;
      })
      .join("");
    return `
      <section class="nav-group">
        <h3>${escapeHtml(pathname)}</h3>
        <div class="nav-group-links">${operationLinks}</div>
      </section>
    `;
  }).join("") || "<p class=\"muted\">No operations match the current filter.</p>";
}

/**
 * @param {any} operation
 * @returns {string}
 */
function renderSecurityBadges(operation) {
  const requirements = operation.security || [];
  if (!Array.isArray(requirements) || requirements.length === 0) return "<span class=\"meta-chip\">public</span>";
  const names = new Set();
  for (const requirement of requirements) {
    for (const schemeName of Object.keys(requirement || {})) names.add(schemeName);
  }
  return Array.from(names).map((name) => `<span class="meta-chip">secured: ${escapeHtml(name)}</span>`).join("");
}

/**
 * @param {string} operationId
 * @param {"path" | "query" | "header"} location
 * @param {Array<any>} parameters
 * @returns {string}
 */
function renderParameterInputs(operationId, location, parameters) {
  if (parameters.length === 0) return "";
  const operationState = state.operations[operationId];
  return `
    <section class="operation-section">
      <div class="section-header">
        <h5>${titleCase(location)} parameters</h5>
      </div>
      <div class="field-grid">
        ${parameters.map((parameter) => {
          const value = operationState.values[location][parameter.name] || "";
          const placeholder = defaultParameterValue(parameter);
          return `
            <label class="field">
              <span>${escapeHtml(parameter.name)} ${parameter.required ? "<em class=\"required\">required</em>" : ""}</span>
              <input
                type="text"
                value="${escapeHtml(value)}"
                placeholder="${escapeHtml(placeholder)}"
                data-operation="${operationId}"
                data-input-kind="parameter"
                data-location="${location}"
                data-name="${escapeHtml(parameter.name)}"
              >
              <small>${escapeHtml(schemaLabel(parameter.schema))}</small>
            </label>
          `;
        }).join("")}
      </div>
      <details class="schema-panel">
        <summary>Schema</summary>
        <pre><code>${prettyJson(parameters)}</code></pre>
      </details>
    </section>
  `;
}

/**
 * @param {string} operationId
 * @param {any} operation
 * @returns {string}
 */
function renderRequestBody(operationId, operation) {
  const content = operation.requestBody?.content || {};
  const contentTypes = Object.keys(content);
  if (contentTypes.length === 0) return "";
  const operationState = state.operations[operationId];
  const activeType = operationState.values.contentType || contentTypes[0];
  const descriptor = content[activeType] || content[contentTypes[0]];
  return `
    <section class="operation-section">
      <div class="section-header">
        <h5>Request body ${operation.requestBody?.required ? "<em class=\"required\">required</em>" : ""}</h5>
      </div>
      <div class="field-grid body-grid">
        <label class="field">
          <span>Content type</span>
          <select data-operation="${operationId}" data-input-kind="content-type">
            ${contentTypes.map((contentType) => `<option value="${escapeHtml(contentType)}" ${contentType === activeType ? "selected" : ""}>${escapeHtml(contentType)}</option>`).join("")}
          </select>
        </label>
      </div>
      <label class="field">
        <span>Body editor</span>
        <textarea
          rows="10"
          data-operation="${operationId}"
          data-input-kind="body"
          spellcheck="false"
        >${escapeHtml(operationState.values.body)}</textarea>
      </label>
      <details class="schema-panel">
        <summary>Schema</summary>
        <pre><code>${prettyJson(descriptor?.schema || {})}</code></pre>
      </details>
    </section>
  `;
}

/**
 * @param {OperationState["response"]} response
 * @returns {string}
 */
function renderResponse(response) {
  if (!response) {
    return "<p class=\"muted\">Run the operation to inspect the live response.</p>";
  }
  return `
    <div class="response-meta">
      <span class="badge ${response.ok ? "badge-success" : "badge-error"}">${response.status} ${escapeHtml(response.statusText)}</span>
      <span class="meta-chip">${escapeHtml(response.contentType || "unknown content type")}</span>
      <span class="meta-chip">${response.durationMs} ms</span>
    </div>
    <details class="schema-panel" open>
      <summary>Response body</summary>
      <pre><code>${escapeHtml(response.bodyText || "")}</code></pre>
    </details>
    <details class="schema-panel">
      <summary>Response headers</summary>
      <pre><code>${prettyJson(Object.fromEntries(response.headers))}</code></pre>
    </details>
  `;
}

/** @returns {[string, Record<string, any>][]} */
function visibleRouteEntries() {
  const filter = state.filter.trim().toLowerCase();
  const filterTerms = filter.split(/\s+/).filter(Boolean);
  const routeEntries = /** @type {[string, Record<string, any>][]} */ (Object.entries(state.spec?.paths || {}));
  if (!filter) return routeEntries;
  return routeEntries
    .map(([pathname, operations]) => {
      /** @type {Record<string, any>} */
      const nextOperations = {};
      for (const [method, operation] of Object.entries(operations)) {
        const haystack = [
          pathname,
          method,
          operation.operationId,
          operation.summary,
          operation.description,
          ...(operation.tags || []),
        ]
          .filter(Boolean)
          .join(" ")
          .toLowerCase();
        if (filterTerms.every((term) => haystack.includes(term))) nextOperations[method] = operation;
      }
      return /** @type {[string, Record<string, any>]} */ ([pathname, nextOperations]);
    })
    .filter(([, operations]) => Object.keys(operations).length > 0);
}

function renderOperations() {
  if (!routesRoot) return;
  const routeEntries = visibleRouteEntries();
  renderNav(routeEntries);
  routesRoot.innerHTML = routeEntries.map(([pathname, operations]) => {
    const operationMarkup = methodOrder
      .filter((method) => operations[method])
      .map((method) => {
        const operation = operations[method];
        const operationId = operationSlug(pathname, method);
        const operationState = ensureOperationState(operationId, operation);
        const pathParameters = (operation.parameters || []).filter(/** @param {any} parameter */ (parameter) => parameter.in === "path");
        const queryParameters = (operation.parameters || []).filter(/** @param {any} parameter */ (parameter) => parameter.in === "query");
        const headerParameters = (operation.parameters || []).filter(/** @param {any} parameter */ (parameter) => parameter.in === "header");
        const contentTypes = Object.keys(operation.requestBody?.content || {});
        const requestBodyActions = contentTypes.length > 0 || pathParameters.length > 0 || queryParameters.length > 0 || headerParameters.length > 0;
        return `
          <article class="operation-card ${operationState.expanded ? "is-open" : ""}" id="${operationId}">
            <button class="operation-toggle" type="button" data-action="toggle-operation" data-operation="${operationId}">
              <span class="method-pill method-${method}">${escapeHtml(method)}</span>
              <span class="operation-path">${escapeHtml(pathname)}</span>
              <span class="operation-title">${escapeHtml(operation.summary || operation.operationId || "Documented operation")}</span>
            </button>
            <div class="operation-body" ${operationState.expanded ? "" : "hidden"}>
              <div class="operation-meta">
                ${renderSecurityBadges(operation)}
                ${(operation.tags || []).map(/** @param {string} tag */ (tag) => `<span class="meta-chip">${escapeHtml(tag)}</span>`).join("")}
              </div>
              ${operation.description ? `<p class="operation-description">${escapeHtml(operation.description)}</p>` : ""}
              <div class="operation-toolbar">
                <button type="button" class="button button-secondary" data-action="toggle-try" data-operation="${operationId}">
                  ${operationState.tryItOut ? "Cancel" : "Try it out"}
                </button>
                <button type="button" class="button button-secondary" data-action="copy-curl" data-operation="${operationId}" ${operationState.loading ? "disabled" : ""}>
                  Copy cURL
                </button>
                ${requestBodyActions ? `<button type="button" class="button button-secondary" data-action="reset-form" data-operation="${operationId}" ${operationState.loading ? "disabled" : ""}>Reset</button>` : ""}
                <button type="button" class="button button-primary" data-action="execute" data-operation="${operationId}" ${operationState.loading ? "disabled" : ""}>
                  ${operationState.loading ? "Running…" : "Execute"}
                </button>
              </div>
              <fieldset class="try-out-shell" ${operationState.tryItOut ? "" : "disabled"}>
                ${renderParameterInputs(operationId, "path", pathParameters)}
                ${renderParameterInputs(operationId, "query", queryParameters)}
                ${renderParameterInputs(operationId, "header", headerParameters)}
                ${renderRequestBody(operationId, operation)}
              </fieldset>
              <section class="operation-section">
                <div class="section-header">
                  <h5>Documented responses</h5>
                </div>
                <details class="schema-panel">
                  <summary>Open response schemas</summary>
                  <pre><code>${prettyJson(operation.responses || {})}</code></pre>
                </details>
              </section>
              <section class="operation-section">
                <div class="section-header">
                  <h5>Live response</h5>
                </div>
                <div class="live-response" data-response="${operationId}">
                  ${renderResponse(operationState.response)}
                </div>
              </section>
            </div>
          </article>
        `;
      })
      .join("");
    return `
      <article class="route-card">
        <div class="route-header">
          <div>
            <span class="label">Route</span>
            <h3 class="path">${escapeHtml(pathname)}</h3>
          </div>
          <span class="badge">${Object.keys(operations).length} operations</span>
        </div>
        <div class="operation-list">${operationMarkup}</div>
      </article>
    `;
  }).join("") || `
    <article class="route-card">
      <p class="muted">No operations match the current filter.</p>
    </article>
  `;
}

function renderAuthPanel() {
  if (!authRoot) return;
  const schemes = state.spec?.components?.securitySchemes || {};
  authRoot.innerHTML = renderAuthFields(schemes);
}

function renderServerSelect() {
  const servers = Array.isArray(state.spec?.servers) && state.spec.servers.length > 0
    ? state.spec.servers
    : [{ url: window.location.origin }];
  if (!state.server) state.server = String(servers[0].url || window.location.origin);
  if (!serverSelect || !originRoot) return;
  serverSelect.innerHTML = servers.map(/** @param {{ url?: string, description?: string }} server */ (server) => {
    const url = String(server.url || window.location.origin);
    const description = typeof server.description === "string" && server.description
      ? ` — ${server.description}`
      : "";
    return `<option value="${escapeHtml(url)}" ${url === state.server ? "selected" : ""}>${escapeHtml(url + description)}</option>`;
  }).join("");
  originRoot.textContent = state.server;
}

function renderAll() {
  if (!state.spec) return;
  if (filterInput && filterInput.value !== state.filter) filterInput.value = state.filter;
  renderSummary(state.spec);
  renderServerSelect();
  renderAuthPanel();
  renderOperations();
  persistState();
}

/**
 * @param {string} pathname
 * @param {any} operation
 * @param {OperationState} operationState
 */
function buildExecution(pathname, operation, operationState) {
  const base = new URL(state.server || window.location.origin, window.location.origin);
  let resolvedPath = pathname;
  for (const [name, value] of Object.entries(operationState.values.path)) {
    resolvedPath = resolvedPath.replaceAll(`{${name}}`, encodeURIComponent(value));
  }
  const url = new URL(resolvedPath, base);
  for (const [name, value] of Object.entries(operationState.values.query)) {
    if (value !== "") url.searchParams.set(name, value);
  }
  /** @type {Record<string, string>} */
  const headers = {};
  for (const [name, value] of Object.entries(operationState.values.header)) {
    if (value !== "") headers[name] = value;
  }
  const responseContentTypes = Object.values(operation.responses || {})
    .flatMap((response) => Object.keys(response?.content || {}));
  if (responseContentTypes.length > 0) {
    headers.accept = responseContentTypes[0];
  } else {
    headers.accept = "application/json, text/plain;q=0.9, */*;q=0.1";
  }
  applySecurity(operation, headers, url);

  /** @type {RequestInit} */
  const init = {
    method: findOperationMethod(pathname, operation),
    headers,
    credentials: "same-origin",
  };
  const rawBody = operationState.values.body;
  if (!["GET", "HEAD"].includes(init.method || "") && rawBody.trim() !== "") {
    const contentType = operationState.values.contentType;
    const payload = buildRequestPayload(contentType, rawBody);
    if (payload !== null) {
      if (!(payload instanceof FormData) && contentType) headers["content-type"] = contentType;
      init.body = payload;
    }
  }
  return { url, init };
}

/**
 * @param {string} contentType
 * @param {string} rawBody
 * @returns {BodyInit | null}
 */
function buildRequestPayload(contentType, rawBody) {
  if (!rawBody.trim()) return null;
  if (contentType.includes("application/json")) {
    const parsed = JSON.parse(rawBody);
    return JSON.stringify(parsed);
  }
  if (contentType.includes("application/x-www-form-urlencoded")) {
    const parsed = JSON.parse(rawBody);
    return new URLSearchParams(parsed).toString();
  }
  if (contentType.includes("multipart/form-data")) {
    const parsed = JSON.parse(rawBody);
    const formData = new FormData();
    for (const [key, value] of Object.entries(parsed || {})) {
      formData.append(key, value == null ? "" : String(value));
    }
    return formData;
  }
  return rawBody;
}

/**
 * @param {any} operation
 * @param {Record<string, string>} headers
 * @param {URL} url
 */
function applySecurity(operation, headers, url) {
  const requirements = operation.security || state.spec?.security || [];
  if (!Array.isArray(requirements) || requirements.length === 0) return;
  for (const requirement of requirements) {
    const schemeNames = Object.keys(requirement || {});
    if (schemeNames.length === 0) return;
    let applied = true;
    for (const schemeName of schemeNames) {
      const scheme = state.spec?.components?.securitySchemes?.[schemeName];
      const values = state.auth[schemeName] || {};
      if (!scheme) {
        applied = false;
        break;
      }
      if (scheme.type === "http" && scheme.scheme === "basic") {
        if (!values.username || !values.password) {
          applied = false;
          break;
        }
        headers.authorization = `Basic ${btoa(`${values.username}:${values.password}`)}`;
      } else if (scheme.type === "http" && scheme.scheme === "bearer") {
        if (!values.token) {
          applied = false;
          break;
        }
        headers.authorization = `Bearer ${values.token}`;
      } else if (scheme.type === "apiKey") {
        if (!values.value) {
          applied = false;
          break;
        }
        if (scheme.in === "query") {
          url.searchParams.set(String(scheme.name || "api_key"), values.value);
        } else if (scheme.in === "header") {
          headers[String(scheme.name || "x-api-key")] = values.value;
        } else {
          applied = false;
          break;
        }
      } else {
        applied = false;
        break;
      }
    }
    if (applied) return;
  }
}

/**
 * @param {string} pathname
 * @param {any} operation
 * @returns {string}
 */
function findOperationMethod(pathname, operation) {
  const operations = state.spec?.paths?.[pathname] || {};
  for (const [method, candidate] of Object.entries(operations)) {
    if (candidate === operation) return method.toUpperCase();
  }
  return "GET";
}

/**
 * @param {string} pathname
 * @param {any} operation
 * @param {OperationState} operationState
 * @returns {string}
 */
function buildCurl(pathname, operation, operationState) {
  const { url, init } = buildExecution(pathname, operation, operationState);
  /** @type {string[]} */
  const parts = ["curl", "-X", init.method || "GET", `'${url.toString()}'`];
  if (init.headers && !(init.headers instanceof Headers)) {
    for (const [name, value] of Object.entries(init.headers)) {
      parts.push("-H", `'${String(name)}: ${String(value).replaceAll("'", "'\\''")}'`);
    }
  }
  if (typeof init.body === "string" && init.body.length > 0) {
    parts.push("--data-raw", `'${init.body.replaceAll("'", "'\\''")}'`);
  }
  return parts.join(" ");
}

/**
 * @param {string} operationId
 * @returns {[string, any, OperationState] | null}
 */
function locateOperation(operationId) {
  for (const [pathname, operations] of Object.entries(state.spec?.paths || {})) {
    for (const [method, operation] of Object.entries(operations)) {
      if (operationSlug(pathname, method) === operationId) {
        return [pathname, operation, ensureOperationState(operationId, operation)];
      }
    }
  }
  return null;
}

/**
 * @param {string} operationId
 * @returns {Promise<void>}
 */
async function executeOperation(operationId) {
  const located = locateOperation(operationId);
  if (!located) return;
  const [pathname, operation, operationState] = located;
  operationState.loading = true;
  renderOperations();
  try {
    const startedAt = performance.now();
    const { url, init } = buildExecution(pathname, operation, operationState);
    const response = await fetch(url, init);
    const durationMs = Math.round(performance.now() - startedAt);
    const contentType = response.headers.get("content-type") || "";
    const bodyText = contentType.includes("application/json")
      ? JSON.stringify(await response.json(), null, 2)
      : await response.text();
    operationState.response = {
      ok: response.ok,
      status: response.status,
      statusText: response.statusText,
      durationMs,
      contentType,
      headers: Array.from(response.headers.entries()),
      bodyText,
    };
  } catch (error) {
    operationState.response = {
      ok: false,
      status: 0,
      statusText: "Request failed",
      durationMs: 0,
      contentType: "text/plain",
      headers: [],
      bodyText: error instanceof Error ? error.message : "Unknown error",
    };
  } finally {
    operationState.loading = false;
    operationState.expanded = true;
    renderOperations();
    const target = document.getElementById(operationId);
    if (target) target.scrollIntoView({ block: "start", behavior: "smooth" });
  }
}

function syncHashSelection() {
  const hash = window.location.hash.replace(/^#/, "");
  if (!hash) return;
  for (const [operationId, operationState] of Object.entries(state.operations)) {
    operationState.expanded = operationId === hash || operationState.expanded;
  }
  renderOperations();
  const target = document.getElementById(hash);
  if (target) target.scrollIntoView({ block: "start" });
}

document.addEventListener("click", async (event) => {
  const target = event.target instanceof HTMLElement ? event.target.closest("[data-action]") : null;
  if (!(target instanceof HTMLElement)) return;
  const action = target.dataset.action;
  const operationId = target.dataset.operation || "";
  if (action === "toggle-operation") {
    const located = locateOperation(operationId);
    if (!located) return;
    const [, , operationState] = located;
    operationState.expanded = !operationState.expanded;
    if (operationState.expanded) history.replaceState(null, "", `#${operationId}`);
    renderOperations();
    return;
  }
  if (action === "toggle-try") {
    const located = locateOperation(operationId);
    if (!located) return;
    const [, , operationState] = located;
    operationState.tryItOut = !operationState.tryItOut;
    operationState.expanded = true;
    renderOperations();
    return;
  }
  if (action === "reset-form") {
    const located = locateOperation(operationId);
    if (!located) return;
    const [, operation] = located;
    delete state.operations[operationId];
    ensureOperationState(operationId, operation).expanded = true;
    renderOperations();
    return;
  }
  if (action === "execute") {
    await executeOperation(operationId);
    return;
  }
  if (action === "copy-curl") {
    const located = locateOperation(operationId);
    if (!located) return;
    const [pathname, operation, operationState] = located;
    const curl = buildCurl(pathname, operation, operationState);
    await navigator.clipboard.writeText(curl);
    target.textContent = "Copied";
    window.setTimeout(() => {
      target.textContent = "Copy cURL";
    }, 1200);
    return;
  }
  if (action === "expand-all") {
    for (const operationState of Object.values(state.operations)) operationState.expanded = true;
    renderOperations();
    return;
  }
  if (action === "collapse-all") {
    for (const operationState of Object.values(state.operations)) operationState.expanded = false;
    renderOperations();
    return;
  }
  if (action === "clear-auth") {
    state.auth = {};
    renderAuthPanel();
    persistState();
  }
});

document.addEventListener("input", (event) => {
  const target = event.target;
  if (!(target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement)) return;
  if (target.id === "docs-filter") {
    state.filter = target.value;
    renderOperations();
    persistState();
    return;
  }
  if (target.dataset.authScheme && target.dataset.authField) {
    const schemeName = target.dataset.authScheme;
    const fieldName = target.dataset.authField;
    if (!state.auth[schemeName]) state.auth[schemeName] = {};
    state.auth[schemeName][fieldName] = target.value;
    persistState();
    return;
  }
  if (target instanceof HTMLSelectElement && target.id === "docs-server") {
    state.server = target.value;
    if (originRoot) originRoot.textContent = state.server;
    persistState();
    return;
  }
  const operationId = target.dataset.operation;
  if (!operationId) return;
  const located = locateOperation(operationId);
  if (!located) return;
  const [, operation, operationState] = located;
  if (target.dataset.inputKind === "parameter") {
    /** @type {"path" | "query" | "header"} */
    const location = /** @type {"path" | "query" | "header"} */ (target.dataset.location || "query");
    const name = target.dataset.name || "";
    operationState.values[location][name] = target.value;
    return;
  }
  if (target.dataset.inputKind === "body") {
    operationState.values.body = target.value;
    return;
  }
  if (target.dataset.inputKind === "content-type" && target instanceof HTMLSelectElement) {
    operationState.values.contentType = target.value;
    const descriptor = operation.requestBody?.content?.[target.value];
    const existingBody = operationState.values.body.trim();
    if (!existingBody) {
      const example = exampleFromSchema(descriptor?.schema || {});
      operationState.values.body = typeof example === "string" ? example : JSON.stringify(example, null, 2);
      renderOperations();
    }
  }
});

window.addEventListener("hashchange", syncHashSelection);

async function loadDocs() {
  try {
    loadPersistedState();
    const response = await fetch(OPENAPI_PATH, { headers: { accept: "application/json" } });
    if (!response.ok) throw new Error(`OpenAPI request failed with ${response.status}`);
    state.spec = await response.json();
    document.title = `${state.spec.info?.title || "Tachyon API"} Docs`;
    if (filterInput) filterInput.value = state.filter;
    if (!state.server) {
      const serverUrl = state.spec?.servers?.[0]?.url;
      state.server = typeof serverUrl === "string" && serverUrl.length > 0 ? serverUrl : window.location.origin;
    }
    renderAll();
    syncHashSelection();
  } catch (error) {
    if (routesRoot) routesRoot.innerHTML = `
      <article class="route-card">
        <strong>Unable to load API docs</strong>
        <p class="muted">${escapeHtml(error instanceof Error ? error.message : "Unknown error")}</p>
      </article>
    `;
    if (summaryRoot) summaryRoot.innerHTML = `
      <article class="summary-card">
        <span class="summary-label">Status</span>
        <strong>Load failed</strong>
      </article>
    `;
  }
}

if (docsRoot) void loadDocs();
