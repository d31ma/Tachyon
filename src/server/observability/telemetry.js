// @ts-check
import path from 'path';
import Fylo from '@d31ma/fylo';
import { fyloOptions } from '../fylo-options.js';
import logger from './logger.js';

/**
 * @typedef {{ traceId: string, spanId: string, traceFlags: string, traceState: string }} TraceContext
 * @typedef {{
 *   traceId: string,
 *   spanId: string,
 *   parentSpanId: string | null,
 *   traceFlags: string,
 *   traceState: string,
 *   name: string,
 *   kind: 'server' | 'internal',
 *   startedAtMs: number,
 *   attributes: Record<string, unknown>,
 *   events: Array<{ name: string, timestampMs: number, attributes?: Record<string, unknown> }>,
 * }} SpanHandle
 * @typedef {{
 *   resourceSpans: Array<{
 *     resource: {
 *       attributes: Array<{ key: string, value: Record<string, unknown> }>,
 *       droppedAttributesCount: number,
 *     },
 *     scopeSpans: Array<{
 *       scope: {
 *         name: string,
 *         version: string,
 *         attributes: Array<{ key: string, value: Record<string, unknown> }>,
 *         droppedAttributesCount: number,
 *       },
 *       spans: Array<Record<string, unknown>>,
 *       schemaUrl?: string,
 *     }>,
 *     schemaUrl?: string,
 *   }>,
 * }} TracesData
 * @typedef {{
 *   schema: 'otlp.json.tracesdata',
 *   requestId: string | null,
 *   traceId: string,
 *   spanId: string,
 *   parentSpanId: string,
 *   kind: number,
 *   name: string,
 *   startTimeUnixNano: string,
 *   endTimeUnixNano: string,
 *   otlpJson: string,
 * }} PersistedTraceDocument
 */

const telemetryLogger = logger.child({ scope: 'telemetry' });
const TELEMETRY_SCOPE_NAME = '@d31ma/tachyon.telemetry';
const SPAN_KIND = {
    internal: 1,
    server: 2,
};
const STATUS_CODE = {
    unset: 0,
    ok: 1,
    error: 2,
};

class TelemetryConfig {
    static collectionName = process.env.YON_OTEL_COLLECTION || 'otel-spans';

    /**
     * @param {string | undefined | null} value
     * @returns {boolean}
     */
    static isTruthy(value) {
        const normalized = value?.trim().toLowerCase();
        return normalized === '1' || normalized === 'true' || normalized === 'yes' || normalized === 'on';
    }

    /**
     * @returns {boolean}
     */
    static enabled() {
        return TelemetryConfig.isTruthy(process.env.YON_OTEL_ENABLED);
    }

    /**
     * @returns {string}
     */
    static root() {
        return process.env.YON_OTEL_ROOT || path.join(process.cwd(), '.tachyon-otel');
    }

    /**
     * @returns {string}
     */
    static serviceName() {
        return process.env.YON_OTEL_SERVICE_NAME || '@d31ma/tachyon';
    }

    /**
     * @returns {string}
     */
    static serviceVersion() {
        return process.env.YON_OTEL_SERVICE_VERSION || process.env.npm_package_version || '2.0.0';
    }

    /**
     * @returns {boolean}
     */
    static shouldCaptureIp() {
        return TelemetryConfig.isTruthy(process.env.YON_OTEL_CAPTURE_IP);
    }
}

class TraceContextParser {
    /**
     * @param {number} length
     * @returns {string}
     */
    static randomHex(length) {
        const bytes = new Uint8Array(Math.ceil(length / 2));
        crypto.getRandomValues(bytes);
        return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('').slice(0, length);
    }

    /**
     * @returns {string}
     */
    static traceId() {
        let value = TraceContextParser.randomHex(32);
        if (/^0+$/.test(value)) {
            value = `1${value.slice(1)}`;
        }
        return value;
    }

    /**
     * @returns {string}
     */
    static spanId() {
        let value = TraceContextParser.randomHex(16);
        if (/^0+$/.test(value)) {
            value = `1${value.slice(1)}`;
        }
        return value;
    }

    /**
     * @param {string | null | undefined} value
     * @returns {TraceContext | null}
     */
    static parseTraceparent(value) {
        if (!value) {
            return null;
        }
        const match = value.trim().match(/^([0-9a-f]{2})-([0-9a-f]{32})-([0-9a-f]{16})-([0-9a-f]{2})$/i);
        if (!match) {
            return null;
        }
        const [, _version, traceId, spanId, traceFlags] = match;
        if (/^0+$/.test(traceId) || /^0+$/.test(spanId)) {
            return null;
        }
        return {
            traceId: traceId.toLowerCase(),
            spanId: spanId.toLowerCase(),
            traceFlags: traceFlags.toLowerCase(),
            traceState: '',
        };
    }
}

class TelemetrySanitizer {
    /**
     * @param {unknown} value
     * @returns {unknown}
     */
    static value(value) {
        if (value === null || value === undefined) {
            return undefined;
        }
        if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
            return value;
        }
        if (typeof value === 'bigint') {
            return value.toString();
        }
        if (value instanceof Date) {
            return value.toISOString();
        }
        if (Array.isArray(value)) {
            return value.map((entry) => TelemetrySanitizer.value(entry));
        }
        if (value instanceof Error) {
            return {
                name: value.name,
                message: value.message,
            };
        }
        if (typeof value === 'object') {
            /** @type {Record<string, unknown>} */
            const next = {};
            for (const [key, entry] of Object.entries(value)) {
                const sanitized = TelemetrySanitizer.value(entry);
                if (sanitized !== undefined) {
                    next[key] = sanitized;
                }
            }
            return next;
        }
        return String(value);
    }

    /**
     * @param {Record<string, unknown> | undefined} attributes
     * @returns {Record<string, unknown>}
     */
    static attributes(attributes) {
        /** @type {Record<string, unknown>} */
        const next = {};
        for (const [key, value] of Object.entries(attributes || {})) {
            const sanitized = TelemetrySanitizer.value(value);
            if (sanitized !== undefined) {
                next[key] = sanitized;
            }
        }
        return next;
    }
}

class TelemetryStore {
    /**
     * @param {PersistedTraceDocument} _span
     * @returns {Promise<void>}
     */
    async persistSpan(_span) {
        throw new Error('TelemetryStore.persistSpan() must be implemented by a subclass');
    }
}

class FyloTelemetryStore extends TelemetryStore {
    /** @type {Promise<InstanceType<typeof Fylo>> | null} */
    static fyloPromise = null;
    /** @type {Promise<void> | null} */
    static ensureCollectionPromise = null;

    /**
     * @returns {Promise<InstanceType<typeof Fylo>>}
     */
    async getFylo() {
        if (!FyloTelemetryStore.fyloPromise) {
            FyloTelemetryStore.fyloPromise = Promise.resolve(new Fylo(fyloOptions(TelemetryConfig.root())));
        }
        return await FyloTelemetryStore.fyloPromise;
    }

    /**
     * @returns {Promise<void>}
     */
    async ensureCollection() {
        if (!FyloTelemetryStore.ensureCollectionPromise) {
            FyloTelemetryStore.ensureCollectionPromise = (async () => {
                const fylo = await this.getFylo();
                try {
                    await fylo.createCollection(TelemetryConfig.collectionName);
                }
                catch (error) {
                    const message = error instanceof Error ? error.message : String(error);
                    if (!/exist|already/i.test(message)) {
                        throw error;
                    }
                }
            })();
        }
        await FyloTelemetryStore.ensureCollectionPromise;
    }

    /**
     * @param {PersistedTraceDocument} span
     * @returns {Promise<void>}
     */
    async persistSpan(span) {
        await this.ensureCollection();
        const fylo = await this.getFylo();
        await fylo.putData(TelemetryConfig.collectionName, span);
    }
}

class SpanFactory {
    /**
     * @returns {Record<string, unknown>}
     */
    static resourceAttributes() {
        return TelemetrySanitizer.attributes({
            'service.name': TelemetryConfig.serviceName(),
            'service.version': TelemetryConfig.serviceVersion(),
            'telemetry.sdk.name': 'tachyon',
            'telemetry.sdk.language': 'javascript',
            'telemetry.sdk.version': TelemetryConfig.serviceVersion(),
        });
    }

    /**
     * @param {Record<string, unknown>} attributes
     * @param {string | null} requestId
     * @returns {Record<string, unknown>}
     */
    static spanAttributes(attributes, requestId) {
        return TelemetrySanitizer.attributes({
            ...attributes,
            'tachyon.request.id': requestId ?? undefined,
        });
    }

    /**
     * @param {string} value
     * @returns {number}
     */
    static flagsFromTraceFlags(value) {
        const parsed = Number.parseInt(value, 16);
        return Number.isFinite(parsed) ? parsed & 0xff : 0;
    }

    /**
     * @param {number} timestampMs
     * @returns {string}
     */
    static unixNano(timestampMs) {
        return String(Math.trunc(timestampMs * 1_000_000));
    }

    /**
     * @param {unknown} value
     * @returns {Record<string, unknown>}
     */
    static anyValue(value) {
        if (typeof value === 'string') {
            return { stringValue: value };
        }
        if (typeof value === 'boolean') {
            return { boolValue: value };
        }
        if (typeof value === 'number') {
            if (Number.isInteger(value)) {
                return { intValue: String(value) };
            }
            return { doubleValue: value };
        }
        if (typeof value === 'bigint') {
            return { intValue: value.toString() };
        }
        if (Array.isArray(value)) {
            return {
                arrayValue: {
                    values: value
                        .map((entry) => TelemetrySanitizer.value(entry))
                        .filter((entry) => entry !== undefined)
                        .map((entry) => SpanFactory.anyValue(entry)),
                },
            };
        }
        if (value && typeof value === 'object') {
            return {
                kvlistValue: {
                    values: SpanFactory.keyValues(/** @type {Record<string, unknown>} */ (value)),
                },
            };
        }
        return { stringValue: String(value) };
    }

    /**
     * @param {Record<string, unknown>} attributes
     * @returns {Array<{ key: string, value: Record<string, unknown> }>}
     */
    static keyValues(attributes) {
        return Object.entries(TelemetrySanitizer.attributes(attributes))
            .map(([key, value]) => value === undefined ? null : {
                key,
                value: SpanFactory.anyValue(value),
            })
            .filter((entry) => entry !== null);
    }

    /**
     * @param {number} statusCode
     * @param {string | undefined} message
     * @returns {{ code: number, message?: string }}
     */
    static status(statusCode, message) {
        if (statusCode >= 500) {
            return {
                code: STATUS_CODE.error,
                message: message || 'ERROR',
            };
        }
        if (statusCode > 0) {
            return {
                code: STATUS_CODE.ok,
            };
        }
        return {
            code: STATUS_CODE.unset,
        };
    }

    /**
     * @param {Request} request
     * @param {{ requestId: string, route: string, method: string, path: string, protocol: string, host: string, ipAddress?: string }} input
     * @returns {SpanHandle | null}
     */
    static createRequestSpan(request, input) {
        if (!TelemetryConfig.enabled()) {
            return null;
        }
        const upstream = TraceContextParser.parseTraceparent(request.headers.get('traceparent'));
        return {
            traceId: upstream?.traceId || TraceContextParser.traceId(),
            spanId: TraceContextParser.spanId(),
            parentSpanId: upstream?.spanId ?? null,
            traceFlags: upstream?.traceFlags ?? '01',
            traceState: request.headers.get('tracestate')?.trim() || upstream?.traceState || '',
            name: `${input.method} ${input.route}`,
            kind: 'server',
            startedAtMs: Date.now(),
            attributes: SpanFactory.spanAttributes({
                'http.request.method': input.method,
                'http.route': input.route,
                'url.path': input.path,
                'server.address': input.host,
                'url.scheme': input.protocol,
                'client.address': TelemetryConfig.shouldCaptureIp() ? input.ipAddress : undefined,
            }, input.requestId),
            events: [],
        };
    }

    /**
     * @param {SpanHandle | null} parent
     * @param {string} name
     * @param {'internal'} kind
     * @param {string | null} requestId
     * @param {Record<string, unknown>} [attributes]
     * @returns {SpanHandle | null}
     */
    static createSpan(parent, name, kind, requestId, attributes = {}) {
        if (!TelemetryConfig.enabled() || !parent) {
            return null;
        }
        return {
            traceId: parent.traceId,
            spanId: TraceContextParser.spanId(),
            parentSpanId: parent.spanId,
            traceFlags: parent.traceFlags,
            traceState: parent.traceState,
            name,
            kind,
            startedAtMs: Date.now(),
            attributes: SpanFactory.spanAttributes(attributes, requestId),
            events: [],
        };
    }

    /**
     * @param {{ traceId?: string, spanId?: string, traceFlags?: string, traceState?: string } | null | undefined} context
     * @param {string} name
     * @param {'internal'} kind
     * @param {string | null} requestId
     * @param {Record<string, unknown>} [attributes]
     * @returns {SpanHandle | null}
     */
    static createChildSpan(context, name, kind, requestId, attributes = {}) {
        if (!TelemetryConfig.enabled() || !context?.traceId || !context?.spanId) {
            return null;
        }
        return {
            traceId: context.traceId,
            spanId: TraceContextParser.spanId(),
            parentSpanId: context.spanId,
            traceFlags: context.traceFlags || '01',
            traceState: context.traceState || '',
            name,
            kind,
            startedAtMs: Date.now(),
            attributes: SpanFactory.spanAttributes(attributes, requestId),
            events: [],
        };
    }

    /**
     * @param {SpanHandle | null} span
     * @param {string} name
     * @param {Record<string, unknown>} [attributes]
     */
    static addEvent(span, name, attributes = {}) {
        if (!span) {
            return;
        }
        span.events.push({
            name,
            timestampMs: Date.now(),
            attributes: TelemetrySanitizer.attributes(attributes),
        });
    }

    /**
     * @param {SpanHandle | null} span
     * @returns {string | null}
     */
    static traceparent(span) {
        if (!span) {
            return null;
        }
        return `00-${span.traceId}-${span.spanId}-${span.traceFlags}`;
    }

    /**
     * @param {SpanHandle} span
     * @param {{ statusCode: number, statusMessage?: string, attributes?: Record<string, unknown> }} input
     * @returns {TracesData}
     */
    static buildTracesData(span, input) {
        const endedAtMs = Date.now();
        return {
            resourceSpans: [{
                resource: {
                    attributes: SpanFactory.keyValues(SpanFactory.resourceAttributes()),
                    droppedAttributesCount: 0,
                },
                scopeSpans: [{
                    scope: {
                        name: TELEMETRY_SCOPE_NAME,
                        version: TelemetryConfig.serviceVersion(),
                        attributes: [],
                        droppedAttributesCount: 0,
                    },
                    spans: [{
                        traceId: span.traceId,
                        spanId: span.spanId,
                        traceState: span.traceState,
                        parentSpanId: span.parentSpanId || '',
                        flags: SpanFactory.flagsFromTraceFlags(span.traceFlags),
                        name: span.name,
                        kind: SPAN_KIND[span.kind],
                        startTimeUnixNano: SpanFactory.unixNano(span.startedAtMs),
                        endTimeUnixNano: SpanFactory.unixNano(endedAtMs),
                        attributes: SpanFactory.keyValues({
                            ...span.attributes,
                            ...input.attributes,
                        }),
                        droppedAttributesCount: 0,
                        events: span.events.map((event) => ({
                            timeUnixNano: SpanFactory.unixNano(event.timestampMs),
                            name: event.name,
                            attributes: SpanFactory.keyValues(event.attributes || {}),
                            droppedAttributesCount: 0,
                        })),
                        droppedEventsCount: 0,
                        links: [],
                        droppedLinksCount: 0,
                        status: SpanFactory.status(input.statusCode, input.statusMessage),
                    }],
                }],
            }],
        };
    }

    /**
     * @param {SpanHandle} span
     * @param {{ statusCode: number, statusMessage?: string, attributes?: Record<string, unknown> }} input
     * @returns {PersistedTraceDocument}
     */
    static toDocument(span, input) {
        const tracesData = SpanFactory.buildTracesData(span, input);
        const persistedSpan = tracesData.resourceSpans[0]?.scopeSpans[0]?.spans[0] || {};
        const requestId = span.attributes['tachyon.request.id'];
        return {
            schema: 'otlp.json.tracesdata',
            requestId: typeof requestId === 'string' ? requestId : null,
            traceId: span.traceId,
            spanId: span.spanId,
            parentSpanId: span.parentSpanId || '',
            kind: typeof persistedSpan.kind === 'number' ? persistedSpan.kind : 0,
            name: span.name,
            startTimeUnixNano: typeof persistedSpan.startTimeUnixNano === 'string' ? persistedSpan.startTimeUnixNano : '0',
            endTimeUnixNano: typeof persistedSpan.endTimeUnixNano === 'string' ? persistedSpan.endTimeUnixNano : '0',
            otlpJson: JSON.stringify(tracesData),
        };
    }
}

export default class Telemetry {
    static store = new FyloTelemetryStore();

    /**
     * @returns {boolean}
     */
    static enabled() {
        return TelemetryConfig.enabled();
    }

    /**
     * @param {Request} request
     * @param {{ requestId: string, route: string, method: string, path: string, protocol: string, host: string, ipAddress?: string }} input
     * @returns {SpanHandle | null}
     */
    static startRequestSpan(request, input) {
        return SpanFactory.createRequestSpan(request, input);
    }

    /**
     * @param {SpanHandle | null} parent
     * @param {string} name
     * @param {'internal'} kind
     * @param {string | null} requestId
     * @param {Record<string, unknown>} [attributes]
     * @returns {SpanHandle | null}
     */
    static startSpan(parent, name, kind, requestId, attributes = {}) {
        return SpanFactory.createSpan(parent, name, kind, requestId, attributes);
    }

    /**
     * @param {{ traceId?: string, spanId?: string, traceFlags?: string, traceState?: string } | null | undefined} context
     * @param {string} name
     * @param {'internal'} kind
     * @param {string | null} requestId
     * @param {Record<string, unknown>} [attributes]
     * @returns {SpanHandle | null}
     */
    static startChildSpan(context, name, kind, requestId, attributes = {}) {
        return SpanFactory.createChildSpan(context, name, kind, requestId, attributes);
    }

    /**
     * @param {SpanHandle | null} span
     * @param {string} name
     * @param {Record<string, unknown>} [attributes]
     */
    static addEvent(span, name, attributes = {}) {
        SpanFactory.addEvent(span, name, attributes);
    }

    /**
     * @param {SpanHandle | null} span
     * @returns {string | null}
     */
    static traceparent(span) {
        return SpanFactory.traceparent(span);
    }

    /**
     * @param {Response} response
     * @param {SpanHandle | null} span
     * @returns {Response}
     */
    static withTraceHeaders(response, span) {
        const traceparent = Telemetry.traceparent(span);
        if (!traceparent || !span) {
            return response;
        }
        const headers = new Headers(response.headers);
        headers.set('Traceparent', traceparent);
        headers.set('X-Trace-Id', span.traceId);
        return new Response(response.body, {
            status: response.status,
            statusText: response.statusText,
            headers,
        });
    }

    /**
     * @param {SpanHandle | null} span
     * @param {{ statusCode: number, statusMessage?: string, attributes?: Record<string, unknown> }} input
     * @returns {Promise<void>}
     */
    static async endSpan(span, input) {
        if (!span) {
            return;
        }
        const doc = SpanFactory.toDocument(span, input);
        try {
            await Telemetry.store.persistSpan(doc);
        }
        catch (error) {
            telemetryLogger.warn('Failed to persist telemetry span', {
                err: error,
                traceId: span.traceId,
                spanId: span.spanId,
                name: span.name,
            });
        }
    }
}
