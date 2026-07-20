// @ts-check
import { createHash } from 'crypto';
import { readFile } from 'fs/promises';
import path from 'path';
import { pathToFileURL } from 'url';

const DESKTOP_TARGETS = new Set(['macos', 'windows', 'linux']);
const RESERVED_PREFIXES = Object.freeze([
    'app.', 'auth.', 'browser.', 'capabilities.', 'clipboard.', 'contentSurface.',
    'file.', 'fs.', 'haptics.', 'host.', 'media.', 'notify.', 'openUrl',
    'screenCapture.', 'secrets.', 'share.', 'shell.', 'shortcuts.', 'window.',
]);

/** @param {unknown} value @param {string} label */
function requireObject(value, label) {
    if (!value || typeof value !== 'object' || Array.isArray(value))
        throw new Error(`${label} must be an object`);
    return /** @type {Record<string, any>} */ (value);
}

/** @param {unknown} descriptor @param {string} projectRoot */
async function resolveDescriptor(descriptor, projectRoot) {
    const source = requireObject(descriptor, 'native host extension');
    if (source.schemaVersion !== 1)
        throw new Error('native host extension schemaVersion must be 1');
    const id = String(source.id ?? '');
    if (!/^[a-z][a-z0-9-]*$/.test(id))
        throw new Error(`native host extension id '${id}' is invalid`);
    const version = String(source.version ?? '');
    if (!version)
        throw new Error(`native host extension '${id}' requires a version`);
    if (source.bridgeAbiVersion !== 1)
        throw new Error(`native host extension '${id}' requires bridgeAbiVersion 1`);
    if (!Array.isArray(source.operations) || source.operations.length === 0)
        throw new Error(`native host extension '${id}' requires operations`);
    const implementations = requireObject(source.implementations, `native host extension '${id}' implementations`);
    const operationNames = new Set();
    const operations = source.operations.map((rawOperation) => {
        const operation = requireObject(rawOperation, `native host extension '${id}' operation`);
        const name = String(operation.name ?? '');
        if (!name.startsWith(`${id}.`) || RESERVED_PREFIXES.some((prefix) => name.startsWith(prefix)))
            throw new Error(`native host extension operation '${name}' has a reserved name or namespace collision`);
        if (operationNames.has(name))
            throw new Error(`native host extension operation '${name}' is duplicated`);
        operationNames.add(name);
        if (!Array.isArray(operation.targets) || operation.targets.length === 0)
            throw new Error(`native host extension operation '${name}' requires targets`);
        const targets = [...new Set(operation.targets.map(String))].sort();
        if (targets.some((target) => !DESKTOP_TARGETS.has(target)))
            throw new Error(`native host extension operation '${name}' contains an unsupported target`);
        const permissions = Array.isArray(operation.permissions)
            ? [...new Set(operation.permissions.map(String))].sort()
            : [];
        return { name, targets, permissions };
    }).sort((left, right) => left.name.localeCompare(right.name));

    /** @type {Record<string, { source: string, sourcePath: string, factory: string }>} */
    const resolvedImplementations = {};
    const requiredTargets = new Set(operations.flatMap((operation) => operation.targets));
    for (const target of [...requiredTargets].sort()) {
        const implementation = requireObject(implementations[target], `native host extension '${id}' ${target} implementation`);
        const sourcePath = path.resolve(projectRoot, String(implementation.source ?? ''));
        const factory = String(implementation.factory ?? '');
        if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(factory))
            throw new Error(`native host extension '${id}' ${target} implementation requires a factory`);
        resolvedImplementations[target] = { source: await readFile(sourcePath, 'utf8'), sourcePath, factory };
    }
    const hashInput = JSON.stringify({
        schemaVersion: 1,
        id,
        version,
        bridgeAbiVersion: 1,
        operations,
        implementations: Object.fromEntries(Object.entries(resolvedImplementations)
            .sort(([left], [right]) => left.localeCompare(right))
            .map(([target, implementation]) => [target, { factory: implementation.factory, source: implementation.source }])),
    });
    return {
        schemaVersion: 1,
        id,
        version,
        bridgeAbiVersion: 1,
        operations,
        implementations: resolvedImplementations,
        digest: `sha256:${createHash('sha256').update(hashInput).digest('hex')}`,
    };
}

/**
 * Resolves trusted build-time extension modules or already-loaded descriptors.
 * @param {string} projectRoot
 * @param {unknown[]} [specifiers]
 */
export async function resolveNativeHostExtensions(projectRoot, specifiers = []) {
    if (!Array.isArray(specifiers))
        throw new Error('nativeHostExtensions must be an array');
    const descriptors = [];
    for (const specifier of specifiers) {
        let descriptor = specifier;
        if (typeof specifier === 'string') {
            const modulePath = path.resolve(projectRoot, specifier);
            const module = await import(`${pathToFileURL(modulePath).href}?t=${Date.now()}`);
            descriptor = module.default ?? module;
        }
        descriptors.push(await resolveDescriptor(descriptor, projectRoot));
    }
    descriptors.sort((left, right) => left.id.localeCompare(right.id));
    const ids = new Set();
    const operations = new Set();
    /** @type {Map<string, string>} */
    const contributionNames = new Map();
    for (const descriptor of descriptors) {
        if (ids.has(descriptor.id))
            throw new Error(`native host extension id '${descriptor.id}' is duplicated`);
        ids.add(descriptor.id);
        for (const operation of descriptor.operations) {
            if (operations.has(operation.name))
                throw new Error(`native host extension operation '${operation.name}' is duplicated`);
            operations.add(operation.name);
        }
        for (const [target, implementation] of Object.entries(descriptor.implementations)) {
            const contributionKey = `${target}:${path.basename(implementation.sourcePath).toLowerCase()}`;
            const owner = contributionNames.get(contributionKey);
            if (owner)
                throw new Error(`native host extensions '${owner}' and '${descriptor.id}' contribute the same ${target} source filename '${path.basename(implementation.sourcePath)}'`);
            contributionNames.set(contributionKey, descriptor.id);
        }
    }
    return descriptors;
}

export { RESERVED_PREFIXES };
