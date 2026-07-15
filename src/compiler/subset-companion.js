// @ts-check
/**
 * In-house frontends for the portable Tac companion subset. These frontends
 * intentionally compile controller code, not a language runtime: an app can
 * use the same source on every Tachyon target without a target-specific SDK.
 */

/** @typedef {'rust' | 'kotlin' | 'swift' | 'csharp'} TacSubsetLanguage */
/** @typedef {{ name: string, initial: unknown, annotations: TacAnnotation[] }} TacSubsetField */
/** @typedef {{ name: string, parameters: string[], body: string, annotations: TacAnnotation[] }} TacSubsetMethod */
/** @typedef {{ name: string, value: string }} TacAnnotation */

const LANGUAGE_LABELS = Object.freeze({
    rust: 'Rust',
    kotlin: 'Kotlin',
    swift: 'Swift',
    csharp: 'C#',
});

/**
 * Constructs each dialect refuses by name instead of emitting JavaScript that
 * is broken or quietly changes meaning (e.g. Kotlin `for (x in items)` would
 * become JavaScript key iteration). See README "Portable companion subset".
 * @type {Record<TacSubsetLanguage, string[]>}
 */
const UNSUPPORTED_CONSTRUCTS = Object.freeze({
    rust: ['match', 'trait', 'enum', 'mod', 'unsafe', 'loop', 'for', 'dyn', 'where', 'macro_rules'],
    kotlin: ['when', 'for', 'object', 'interface', 'sealed', 'lateinit', 'init'],
    swift: ['guard', 'switch', 'for', 'protocol', 'extension', 'enum', 'struct', 'defer', 'deinit'],
    csharp: ['switch', 'foreach', 'namespace', 'interface', 'struct', 'delegate', 'lock', 'goto', 'yield'],
});

/**
 * Builds a diagnostic pinned to `sourcePath:line:column` when a source index
 * is known, so a dialect error points at the construct instead of the file.
 * @param {string} message @param {string} sourcePath @param {string} [source] @param {number} [index]
 */
function subsetError(message, sourcePath, source, index) {
    if (source === undefined || index === undefined || index < 0)
        return new Error(`${sourcePath}: ${message}`);
    const prefix = source.slice(0, index);
    const line = prefix.split('\n').length;
    const column = index - prefix.lastIndexOf('\n');
    return new Error(`${sourcePath}:${line}:${column}: ${message}`);
}

/**
 * Walks the source once and reports every string literal and comment span so
 * structural passes never read text inside them. A single quote only opens a
 * character literal when it closes as one ('x', '\n'); a lone quote such as a
 * Rust lifetime ('static) is code.
 * @param {string} source @param {(start: number, end: number) => void} onLiteral end is exclusive
 */
function forEachLiteral(source, onLiteral) {
    for (let index = 0; index < source.length; index += 1) {
        const character = source[index];
        const next = source[index + 1];
        if (character === '/' && next === '/') {
            let end = source.indexOf('\n', index);
            if (end < 0) end = source.length;
            onLiteral(index, end);
            index = end;
            continue;
        }
        if (character === '/' && next === '*') {
            const close = source.indexOf('*/', index + 2);
            const end = close < 0 ? source.length : close + 2;
            onLiteral(index, end);
            index = end - 1;
            continue;
        }
        if (character === '"') {
            let end = source.length;
            for (let scan = index + 1; scan < source.length; scan += 1) {
                if (source[scan] === '\\') { scan += 1; continue; }
                if (source[scan] === '"') { end = scan + 1; break; }
            }
            onLiteral(index, end);
            index = end - 1;
            continue;
        }
        if (character === "'" && /^'(?:\\.|[^'\\\n])'/.test(source.slice(index, index + 4))) {
            const end = source[index + 1] === '\\' ? index + 4 : index + 3;
            onLiteral(index, end);
            index = end - 1;
        }
    }
}

/**
 * Replaces literal and comment interiors with spaces. Lengths and newlines
 * are preserved so indexes into the result map 1:1 onto the original source
 * for diagnostics.
 * @param {string} source
 */
function blankLiterals(source) {
    let output = '';
    let last = 0;
    forEachLiteral(source, (start, end) => {
        output += source.slice(last, start) + source.slice(start, end).replace(/[^\n]/g, ' ');
        last = end;
    });
    return output + source.slice(last);
}

/**
 * Extracts literals and comments into placeholder tokens so regex lowering
 * can never rewrite text inside a string such as "navigator.isOnline()".
 * @param {string} source
 * @returns {{ masked: string, restore: (text: string) => string }}
 */
function extractLiterals(source) {
    /** @type {string[]} */
    const saved = [];
    let masked = '';
    let last = 0;
    forEachLiteral(source, (start, end) => {
        masked += `${source.slice(last, start)}\x00${saved.push(source.slice(start, end)) - 1}\x00`;
        last = end;
    });
    masked += source.slice(last);
    return { masked, restore: (text) => text.replace(/\x00(\d+)\x00/g, (_match, position) => saved[Number(position)]) };
}

/** @param {string} blanked @param {TacSubsetLanguage} language @param {string} sourcePath @param {string} source */
function assertSupportedConstructs(blanked, language, sourcePath, source) {
    for (const construct of UNSUPPORTED_CONSTRUCTS[language]) {
        const match = new RegExp(`(?<![.\\w$])${construct}\\b`).exec(blanked);
        if (match)
            throw subsetError(`'${construct}' is not part of the Tac ${LANGUAGE_LABELS[language]} companion subset. Supported control flow is if/else and while; see the portable companion subset reference in the Tachyon README.`, sourcePath, source, match.index);
    }
}

/** @param {string} blanked @param {TacSubsetLanguage} language @param {string} sourcePath @param {string} source */
function assertNoLegacyPrelude(blanked, language, sourcePath, source) {
    /** @type {Partial<Record<TacSubsetLanguage, RegExp>>} */
    const patterns = {
        rust: /\b(?:Web|App|Fylo)::/,
        kotlin: /\b(?:Web|App|Fylo)\./,
        swift: /\b(?:Web|App|Fylo)\./,
        csharp: /\b(?:Web|Application)\./,
    };
    const match = patterns[language]?.exec(blanked);
    if (match)
        throw subsetError(`${LANGUAGE_LABELS[language]} Tac companion uses a removed platform wrapper. Use the implicit language prelude instead.`, sourcePath, source, match.index);
}

/** @param {string} source @param {number} openBrace */
function closingBrace(source, openBrace) {
    let depth = 0;
    let quote = '';
    let lineComment = false;
    let blockComment = false;
    for (let index = openBrace; index < source.length; index += 1) {
        const character = source[index];
        const next = source[index + 1];
        if (lineComment) {
            if (character === '\n') lineComment = false;
            continue;
        }
        if (blockComment) {
            if (character === '*' && next === '/') {
                blockComment = false;
                index += 1;
            }
            continue;
        }
        if (quote) {
            if (character === '\\') {
                index += 1;
                continue;
            }
            if (character === quote) quote = '';
            continue;
        }
        if (character === '/' && next === '/') {
            lineComment = true;
            index += 1;
            continue;
        }
        if (character === '/' && next === '*') {
            blockComment = true;
            index += 1;
            continue;
        }
        if (character === '"' || character === "'") {
            quote = character;
            continue;
        }
        if (character === '{') depth += 1;
        if (character === '}') {
            depth -= 1;
            if (depth === 0) return index;
        }
    }
    throw new Error('Tac companion has an unterminated block.');
}

/** @param {string} source */
function literalValue(source) {
    const value = source.trim();
    if (value === 'null' || value === 'nil') return null;
    if (value === 'true' || value === 'false') return value === 'true';
    if (/^-?(?:0|[1-9]\d*)(?:\.\d+)?$/.test(value)) return Number(value);
    if (/^"(?:\\.|[^"\\])*"$/.test(value)) {
        try { return JSON.parse(value); } catch { return undefined; }
    }
    if (/^'(?:\\.|[^'\\])*'$/.test(value))
        return value.slice(1, -1).replaceAll("\\'", "'").replaceAll('\\\\', '\\');
    if (/^\[(?:[\s\S]*)\]$/.test(value)) {
        try { return JSON.parse(value); } catch { return undefined; }
    }
    return undefined;
}

/** @param {string} text */
function annotations(text) {
    /** @type {TacAnnotation[]} */
    const result = [];
    const pattern = /(?:@|#\[|\[)([A-Za-z_$][\w$]*)(?:\s*\(\s*(['"])(.*?)\2\s*\))?\]?/g;
    for (const match of text.matchAll(pattern)) {
        result.push({ name: match[1].toLowerCase(), value: match[3] ?? '' });
    }
    return result;
}

/** @param {TacAnnotation[]} values @param {string} name */
function annotation(values, name) {
    return values.find((item) => item.name === name)?.value ?? '';
}

/** @param {string} text */
function parameterNames(text) {
    return text.split(',')
        .map((part) => part.trim())
        .filter((part) => part.length > 0 && !/^(?:&?mut\s+)?self$/.test(part))
        .map((part) => {
            const beforeType = part.split(':')[0].trim();
            const words = beforeType.split(/\s+/).filter(Boolean);
            return (words.at(-1) ?? '').replace(/^_\s*/, '');
        })
        .filter((name) => /^[A-Za-z_$][\w$]*$/.test(name));
}

/** @param {number} index @param {Array<{ start: number, end: number }>} methods */
function isMethodMember(index, methods) {
    return methods.some((method) => index >= method.start && index <= method.end);
}

/** @param {string} source @param {TacSubsetLanguage} language @param {string} sourcePath */
function companionClass(source, language, sourcePath) {
    /** @type {Partial<Record<TacSubsetLanguage, RegExp>>} */
    const patterns = {
        kotlin: /\bclass\s+([A-Za-z_$][\w$]*)\s*:\s*Tac(?:\s*\(\s*\))?\s*\{/g,
        swift: /\b(?:final\s+)?class\s+([A-Za-z_$][\w$]*)\s*:\s*Tac\s*\{/g,
        csharp: /\b(?:public\s+)?(?:sealed\s+)?class\s+([A-Za-z_$][\w$]*)\s*:\s*Tac\s*\{/g,
    };
    const pattern = patterns[language];
    if (!pattern) throw new Error(`Tac companion '${sourcePath}' has no class parser for ${language}.`);
    const matches = [...source.matchAll(pattern)];
    if (matches.length !== 1) {
        throw subsetError(`${LANGUAGE_LABELS[language]} Tac companion must declare exactly one class that inherits Tac.`, sourcePath, source, matches[1]?.index);
    }
    const match = matches[0];
    const openBrace = source.indexOf('{', match.index);
    const closeBrace = closingBrace(source, openBrace);
    return { name: match[1], body: source.slice(openBrace + 1, closeBrace), bodyStart: openBrace + 1 };
}

/** @param {TacSubsetLanguage} language */
function methodPattern(language) {
    if (language === 'kotlin')
        return /((?:\s*@[^\n]+\n)*)\s*(?:public\s+|private\s+|internal\s+|protected\s+|open\s+|override\s+|suspend\s+)*fun\s+([A-Za-z_$][\w$]*)\s*\(([^)]*)\)\s*(?::\s*[^\{\n]+)?\s*\{/g;
    if (language === 'swift')
        return /((?:\s*@[^\n]+\n)*)\s*(?:public\s+|private\s+|internal\s+|fileprivate\s+|final\s+|override\s+)*func\s+([A-Za-z_$][\w$]*)\s*\(([^)]*)\)\s*(?:->\s*[^\{\n]+)?\s*\{/g;
    if (language === 'csharp')
        return /((?:\s*\[[^\]]+\]\s*)*)\s*(?:public\s+|private\s+|protected\s+|internal\s+|static\s+|async\s+|virtual\s+|override\s+)*(?:[A-Za-z_$][\w$]*(?:<[^>]+>)?\s+)+([A-Za-z_$][\w$]*)\s*\(([^)]*)\)\s*\{/g;
    return /((?:\s*#\[[^\]]+\]\s*)*)\s*(?:pub\s+)?fn\s+([A-Za-z_$][\w$]*)\s*\(([^)]*)\)\s*(?:->\s*[^\{\n]+)?\s*\{/g;
}

/** @typedef {{ sourcePath: string, source: string, offset: number }} SubsetParseContext */

/** @param {string} body @param {TacSubsetLanguage} language @param {SubsetParseContext} [context] */
function methodsFor(body, language, context) {
    /** @type {TacSubsetMethod[]} */
    const methods = [];
    for (const match of body.matchAll(methodPattern(language))) {
        const openBrace = body.indexOf('{', match.index);
        const closeBrace = closingBrace(body, openBrace);
        const name = match[2];
        if (language === 'rust' && name === 'new') continue;
        methods.push({
            name,
            parameters: parameterNames(match[3]),
            body: body.slice(openBrace + 1, closeBrace),
            annotations: annotations(match[1]),
        });
    }
    if (methods.length === 0)
        throw subsetError(`${LANGUAGE_LABELS[language]} Tac companion must declare at least one method.`, context?.sourcePath ?? '', context?.source);
    return methods;
}

/** @param {string} body @param {TacSubsetLanguage} language @param {TacSubsetMethod[]} methods @param {SubsetParseContext} [context] */
function fieldsFor(body, language, methods, context) {
    /** @type {TacSubsetField[]} */
    const fields = [];
    const ranges = [];
    for (const match of body.matchAll(methodPattern(language))) {
        const openBrace = body.indexOf('{', match.index);
        ranges.push({ start: match.index ?? 0, end: closingBrace(body, openBrace) });
    }
    /** @type {Partial<Record<TacSubsetLanguage, RegExp>>} */
    const patterns = {
        kotlin: /((?:\s*@[^\n]+\n)*)\s*(?:public\s+|private\s+|internal\s+|protected\s+)?(?:var|val)\s+([A-Za-z_$][\w$]*)(?:\s*:\s*[^=;\n]+)?\s*=\s*([^;\n]+)/g,
        swift: /((?:\s*@[^\n]+\n)*)\s*(?:public\s+|private\s+|internal\s+|fileprivate\s+)?(?:var|let)\s+([A-Za-z_$][\w$]*)\s*:\s*[^=;\n]+\s*=\s*([^;\n]+)/g,
        csharp: /((?:\s*\[[^\]]+\]\s*)*)\s*(?:public\s+|private\s+|protected\s+|internal\s+)?(?:readonly\s+)?[A-Za-z_$][\w$]*(?:<[^>]+>)?\s+([A-Za-z_$][\w$]*)\s*=\s*([^;]+);/g,
    };
    const pattern = patterns[language];
    if (!pattern) return fields;
    for (const match of body.matchAll(pattern)) {
        if (isMethodMember(match.index ?? 0, ranges)) continue;
        const initial = literalValue(match[3]);
        if (initial === undefined)
            throw subsetError(`${LANGUAGE_LABELS[language]} Tac companion field '${match[2]}' must have a literal initial value.`, context?.sourcePath ?? '', context?.source, context === undefined ? undefined : context.offset + (match.index ?? 0));
        fields.push({ name: match[2], initial, annotations: annotations(match[1]) });
    }
    return fields;
}

/** @param {string} source @param {string} sourcePath */
function rustCompanion(source, sourcePath) {
    const structMatch = /\bstruct\s+([A-Za-z_$][\w$]*)\s*\{/.exec(source);
    if (!structMatch || structMatch.index === undefined)
        throw subsetError(`Rust Tac companion must declare 'struct Name { ... }'.`, sourcePath, source);
    const openBrace = source.indexOf('{', structMatch.index);
    const closeBrace = closingBrace(source, openBrace);
    const name = structMatch[1];
    const implPattern = new RegExp(`\\bimpl\\s+${name}\\s*\\{`);
    const implMatch = implPattern.exec(source);
    if (!implMatch || implMatch.index === undefined)
        throw subsetError(`Rust Tac companion must declare 'impl ${name} { ... }'.`, sourcePath, source, structMatch.index);
    const implOpenBrace = source.indexOf('{', implMatch.index);
    const implCloseBrace = closingBrace(source, implOpenBrace);
    const structBody = source.slice(openBrace + 1, closeBrace);
    const body = source.slice(implOpenBrace + 1, implCloseBrace);
    const initializers = new Map();
    const newMatch = /\bfn\s+new\s*\([^)]*\)\s*(?:->\s*Self)?\s*\{/.exec(body);
    const newOpenBrace = newMatch?.index === undefined ? -1 : body.indexOf('{', newMatch.index);
    const newMethod = newOpenBrace < 0 ? '' : body.slice(newOpenBrace + 1, closingBrace(body, newOpenBrace));
    const selfInitializer = /Self\s*\{([\s\S]*?)\}/.exec(newMethod)?.[1] ?? '';
    for (const match of selfInitializer.matchAll(/([A-Za-z_$][\w$]*)\s*:\s*([^,\n}]+)/g))
        initializers.set(match[1], match[2]);
    /** @type {TacSubsetField[]} */
    const fields = [];
    for (const match of structBody.matchAll(/((?:\s*#\[[^\]]+\]\s*)*)(?:pub\s+)?([A-Za-z_$][\w$]*)\s*:\s*[^,\n}]+(?:,|$)/gm)) {
        const initial = literalValue(initializers.get(match[2]) ?? '');
        if (initial === undefined)
            throw subsetError(`Rust Tac companion field '${match[2]}' needs a literal value in 'fn new() -> Self { Self { ... } }'.`, sourcePath, source, openBrace + 1 + (match.index ?? 0));
        fields.push({ name: match[2], initial, annotations: annotations(match[1]) });
    }
    return { name, fields, methods: methodsFor(body, 'rust', { sourcePath, source, offset: implOpenBrace + 1 }) };
}

/**
 * Lowers the implicit language prelude into the private Tac runtime bridge.
 * Application code never names Tac or its bridge. Each language receives the
 * same capabilities with its native casing and async conventions.
 * @param {string} output
 * @param {TacSubsetLanguage} language
 */
function translateNativeShims(output, language) {
    if (language === 'rust') {
        return output
            .replace(/\blocal_storage\(\)\.set_item\(\s*([^,()]+)\s*,\s*([^)]+)\)/g, 'this.tac.__native.web.localStorage.setItem($1, $2)')
            .replace(/\blocal_storage\(\)\.get_item\(\s*([^,()]+)\s*,\s*([^)]+)\)/g, 'this.tac.__native.web.localStorage.getItem($1, $2)')
            .replace(/\blocal_storage\(\)\.remove_item\(\s*([^)]+)\)/g, 'this.tac.__native.web.localStorage.removeItem($1)')
            .replace(/\bsession_storage\(\)\.set_item\(\s*([^,()]+)\s*,\s*([^)]+)\)/g, 'this.tac.__native.web.sessionStorage.setItem($1, $2)')
            .replace(/\bsession_storage\(\)\.get_item\(\s*([^,()]+)\s*,\s*([^)]+)\)/g, 'this.tac.__native.web.sessionStorage.getItem($1, $2)')
            .replace(/\bsession_storage\(\)\.remove_item\(\s*([^)]+)\)/g, 'this.tac.__native.web.sessionStorage.removeItem($1)')
            .replace(/\bnavigator\(\)\.language\(\)/g, 'this.tac.__native.web.navigator.language()')
            .replace(/\bnavigator\(\)\.is_online\(\)/g, 'this.tac.__native.web.navigator.online()')
            .replace(/\blocation\(\)\.href\(\)/g, 'this.tac.__native.web.location.href()')
            .replace(/\blocation\(\)\.origin\(\)/g, 'this.tac.__native.web.location.origin()')
            .replace(/\bfetch\(/g, 'this.tac.__native.web.fetch(')
            .replace(/\bfylo\(\)\.collection\(/g, 'this.tac.__native.fylo.collection(')
            .replace(/\bapp\(\)\.is_available\(\)/g, 'this.tac.__native.app.available()')
            .replace(/\bapp\(\)\.info\(\)/g, 'await this.tac.__native.app.info()')
            .replace(/\bclipboard\(\)\.write_text\(\s*([^)]+)\)/g, 'await this.tac.__native.clipboard.writeText($1)')
            .replace(/\bclipboard\(\)\.read_text\(\)/g, 'await this.tac.__native.clipboard.readText()')
            .replace(/\bfile_system\(\)\.read_text\(/g, 'await this.tac.__native.fileSystem.readText(')
            .replace(/\bfile_system\(\)\.write_text\(/g, 'await this.tac.__native.fileSystem.writeText(')
            .replace(/\bfile_system\(\)\.read_dir\(/g, 'await this.tac.__native.fileSystem.readDir(')
            .replace(/\bfile_system\(\)\.stat\(/g, 'await this.tac.__native.fileSystem.stat(')
            .replace(/\bfile_system\(\)\.mkdir\(/g, 'await this.tac.__native.fileSystem.mkdir(')
            .replace(/\bfile_system\(\)\.remove\(/g, 'await this.tac.__native.fileSystem.remove(')
            .replace(/\bfile_system\(\)\.paths\(\)/g, 'await this.tac.__native.fileSystem.paths()')
            .replace(/\bshell\(\)\.exec\(/g, 'await this.tac.__native.shell.exec(')
            .replace(/\bbrowser\(\)\.open\(\s*([^)]+)\)/g, 'await this.tac.__native.browser.open($1)')
            .replace(/\bshare\(\)\.text\(\s*([^)]+)\)/g, 'await this.tac.__native.share.text($1)')
            .replace(/\bhaptics\(\)\.impact\(\)/g, 'await this.tac.__native.haptics.impact()')
            .replace(/\bfile_picker\(\)\.open_text\(\)/g, 'await this.tac.__native.filePicker.openText()')
            .replace(/\bfile_picker\(\)\.save_text\(/g, 'await this.tac.__native.filePicker.saveText(')
            .replace(/\bsecrets\(\)\.get\(/g, 'await this.tac.__native.secrets.get(')
            .replace(/\bsecrets\(\)\.set\(/g, 'await this.tac.__native.secrets.set(')
            .replace(/\bsecrets\(\)\.delete\(/g, 'await this.tac.__native.secrets.delete(')
            .replace(/\bauth\(\)\.verify_user\(/g, 'await this.tac.__native.auth.verifyUser(')
            .replace(/\bgeolocation\(\)\.current\(/g, 'await this.tac.__native.geolocation.current(')
            .replace(/\bnotifications\(\)\.show\(/g, 'await this.tac.__native.notifications.show(')
            .replace(/\bmedia\(\)\.get_user_media\(/g, 'await this.tac.__native.media.getUserMedia(')
            .replace(/\bcapabilities\(\)\.supports\(/g, 'this.tac.__native.capabilities.supports(')
            .replace(/\bcapabilities\(\)\.state\(/g, 'await this.tac.__native.capabilities.state(');
    }
    if (language === 'kotlin') {
        return output
            .replace(/\blocalStorage\./g, 'this.tac.__native.web.localStorage.')
            .replace(/\bsessionStorage\./g, 'this.tac.__native.web.sessionStorage.')
            .replace(/\bnavigator\.language\(\)/g, 'this.tac.__native.web.navigator.language()')
            .replace(/\bnavigator\.isOnline\(\)/g, 'this.tac.__native.web.navigator.online()')
            .replace(/\blocation\.href\(\)/g, 'this.tac.__native.web.location.href()')
            .replace(/\blocation\.origin\(\)/g, 'this.tac.__native.web.location.origin()')
            .replace(/\bfetch\(/g, 'this.tac.__native.web.fetch(')
            .replace(/\bfylo\.collection\(/g, 'this.tac.__native.fylo.collection(')
            .replace(/\bapp\.isAvailable\(\)/g, 'this.tac.__native.app.available()')
            .replace(/\bapp\.info\(\)/g, 'await this.tac.__native.app.info()')
            .replace(/\bclipboard\.writeText\(\s*([^)]+)\)/g, 'await this.tac.__native.clipboard.writeText($1)')
            .replace(/\bclipboard\.readText\(\)/g, 'await this.tac.__native.clipboard.readText()')
            .replace(/\bfileSystem\.readText\(/g, 'await this.tac.__native.fileSystem.readText(')
            .replace(/\bfileSystem\.writeText\(/g, 'await this.tac.__native.fileSystem.writeText(')
            .replace(/\bfileSystem\.readDir\(/g, 'await this.tac.__native.fileSystem.readDir(')
            .replace(/\bfileSystem\.stat\(/g, 'await this.tac.__native.fileSystem.stat(')
            .replace(/\bfileSystem\.mkdir\(/g, 'await this.tac.__native.fileSystem.mkdir(')
            .replace(/\bfileSystem\.remove\(/g, 'await this.tac.__native.fileSystem.remove(')
            .replace(/\bfileSystem\.paths\(\)/g, 'await this.tac.__native.fileSystem.paths()')
            .replace(/\bshell\.exec\(/g, 'await this.tac.__native.shell.exec(')
            .replace(/\bbrowser\.open\(\s*([^)]+)\)/g, 'await this.tac.__native.browser.open($1)')
            .replace(/\bshare\.text\(\s*([^)]+)\)/g, 'await this.tac.__native.share.text($1)')
            .replace(/\bhaptics\.impact\(\)/g, 'await this.tac.__native.haptics.impact()')
            .replace(/\bfilePicker\.openText\(\)/g, 'await this.tac.__native.filePicker.openText()')
            .replace(/\bfilePicker\.saveText\(/g, 'await this.tac.__native.filePicker.saveText(')
            .replace(/\bsecrets\.get\(/g, 'await this.tac.__native.secrets.get(')
            .replace(/\bsecrets\.set\(/g, 'await this.tac.__native.secrets.set(')
            .replace(/\bsecrets\.delete\(/g, 'await this.tac.__native.secrets.delete(')
            .replace(/\bauth\.verifyUser\(/g, 'await this.tac.__native.auth.verifyUser(')
            .replace(/\bgeolocation\.current\(/g, 'await this.tac.__native.geolocation.current(')
            .replace(/\bnotifications\.show\(/g, 'await this.tac.__native.notifications.show(')
            .replace(/\bmedia\.getUserMedia\(/g, 'await this.tac.__native.media.getUserMedia(')
            .replace(/\bcapabilities\.supports\(/g, 'this.tac.__native.capabilities.supports(')
            .replace(/\bcapabilities\.state\(/g, 'await this.tac.__native.capabilities.state(');
    }
    if (language === 'swift') {
        return output
            .replace(/\blocalStorage\./g, 'this.tac.__native.web.localStorage.')
            .replace(/\bsessionStorage\./g, 'this.tac.__native.web.sessionStorage.')
            .replace(/\bnavigator\.language\(\)/g, 'this.tac.__native.web.navigator.language()')
            .replace(/\bnavigator\.isOnline\(\)/g, 'this.tac.__native.web.navigator.online()')
            .replace(/\blocation\.href\(\)/g, 'this.tac.__native.web.location.href()')
            .replace(/\blocation\.origin\(\)/g, 'this.tac.__native.web.location.origin()')
            .replace(/\bfetch\(/g, 'this.tac.__native.web.fetch(')
            .replace(/\bfylo\.collection\(/g, 'this.tac.__native.fylo.collection(')
            .replace(/\bapp\.isAvailable\(\)/g, 'this.tac.__native.app.available()')
            .replace(/\bapp\.info\(\)/g, 'await this.tac.__native.app.info()')
            .replace(/\bclipboard\.writeText\(\s*([^)]+)\)/g, 'await this.tac.__native.clipboard.writeText($1)')
            .replace(/\bclipboard\.readText\(\)/g, 'await this.tac.__native.clipboard.readText()')
            .replace(/\bfileSystem\.readText\(/g, 'await this.tac.__native.fileSystem.readText(')
            .replace(/\bfileSystem\.writeText\(/g, 'await this.tac.__native.fileSystem.writeText(')
            .replace(/\bfileSystem\.readDir\(/g, 'await this.tac.__native.fileSystem.readDir(')
            .replace(/\bfileSystem\.stat\(/g, 'await this.tac.__native.fileSystem.stat(')
            .replace(/\bfileSystem\.mkdir\(/g, 'await this.tac.__native.fileSystem.mkdir(')
            .replace(/\bfileSystem\.remove\(/g, 'await this.tac.__native.fileSystem.remove(')
            .replace(/\bfileSystem\.paths\(\)/g, 'await this.tac.__native.fileSystem.paths()')
            .replace(/\bshell\.exec\(/g, 'await this.tac.__native.shell.exec(')
            .replace(/\bbrowser\.open\(\s*([^)]+)\)/g, 'await this.tac.__native.browser.open($1)')
            .replace(/\bshare\.text\(\s*([^)]+)\)/g, 'await this.tac.__native.share.text($1)')
            .replace(/\bhaptics\.impact\(\)/g, 'await this.tac.__native.haptics.impact()')
            .replace(/\bfilePicker\.openText\(\)/g, 'await this.tac.__native.filePicker.openText()')
            .replace(/\bfilePicker\.saveText\(/g, 'await this.tac.__native.filePicker.saveText(')
            .replace(/\bsecrets\.get\(/g, 'await this.tac.__native.secrets.get(')
            .replace(/\bsecrets\.set\(/g, 'await this.tac.__native.secrets.set(')
            .replace(/\bsecrets\.delete\(/g, 'await this.tac.__native.secrets.delete(')
            .replace(/\bauth\.verifyUser\(/g, 'await this.tac.__native.auth.verifyUser(')
            .replace(/\bgeolocation\.current\(/g, 'await this.tac.__native.geolocation.current(')
            .replace(/\bnotifications\.show\(/g, 'await this.tac.__native.notifications.show(')
            .replace(/\bmedia\.getUserMedia\(/g, 'await this.tac.__native.media.getUserMedia(')
            .replace(/\bcapabilities\.supports\(/g, 'this.tac.__native.capabilities.supports(')
            .replace(/\bcapabilities\.state\(/g, 'await this.tac.__native.capabilities.state(');
    }
    return output
        .replace(/\bLocalStorage\.GetItem\(/g, 'this.tac.__native.web.localStorage.getItem(')
        .replace(/\bLocalStorage\.SetItem\(/g, 'this.tac.__native.web.localStorage.setItem(')
        .replace(/\bLocalStorage\.RemoveItem\(/g, 'this.tac.__native.web.localStorage.removeItem(')
        .replace(/\bSessionStorage\.GetItem\(/g, 'this.tac.__native.web.sessionStorage.getItem(')
        .replace(/\bSessionStorage\.SetItem\(/g, 'this.tac.__native.web.sessionStorage.setItem(')
        .replace(/\bSessionStorage\.RemoveItem\(/g, 'this.tac.__native.web.sessionStorage.removeItem(')
        .replace(/\bNavigator\.Language\(\)/g, 'this.tac.__native.web.navigator.language()')
        .replace(/\bNavigator\.IsOnline\(\)/g, 'this.tac.__native.web.navigator.online()')
        .replace(/\bLocation\.Href\(\)/g, 'this.tac.__native.web.location.href()')
        .replace(/\bLocation\.Origin\(\)/g, 'this.tac.__native.web.location.origin()')
        .replace(/\bFetchAsync\(/g, 'this.tac.__native.web.fetch(')
        .replace(/\bFylo\.Collection\(/g, 'this.tac.__native.fylo.collection(')
        .replace(/(this\.tac\.__native\.fylo\.collection\([^)]*\))\.(Find|Get|Create|Patch|Delete|List|Put|Restore|Latest|Inspect|Rebuild)\(/g, (_match, collection, method) => `${collection}.${method.toLowerCase()}(`)
        .replace(/\bApp\.IsAvailable\(\)/g, 'this.tac.__native.app.available()')
        .replace(/\bApp\.InfoAsync\(\)/g, 'await this.tac.__native.app.info()')
        .replace(/\bClipboard\.SetTextAsync\(\s*([^)]+)\)/g, 'await this.tac.__native.clipboard.writeText($1)')
        .replace(/\bClipboard\.GetTextAsync\(\)/g, 'await this.tac.__native.clipboard.readText()')
        .replace(/\bFileSystem\.ReadTextAsync\(/g, 'await this.tac.__native.fileSystem.readText(')
        .replace(/\bFileSystem\.WriteTextAsync\(/g, 'await this.tac.__native.fileSystem.writeText(')
        .replace(/\bFileSystem\.ReadDirAsync\(/g, 'await this.tac.__native.fileSystem.readDir(')
        .replace(/\bFileSystem\.StatAsync\(/g, 'await this.tac.__native.fileSystem.stat(')
        .replace(/\bFileSystem\.MkdirAsync\(/g, 'await this.tac.__native.fileSystem.mkdir(')
        .replace(/\bFileSystem\.RemoveAsync\(/g, 'await this.tac.__native.fileSystem.remove(')
        .replace(/\bFileSystem\.PathsAsync\(\)/g, 'await this.tac.__native.fileSystem.paths()')
        .replace(/\bShell\.ExecAsync\(/g, 'await this.tac.__native.shell.exec(')
        .replace(/\bBrowser\.OpenAsync\(\s*([^)]+)\)/g, 'await this.tac.__native.browser.open($1)')
        .replace(/\bShare\.TextAsync\(\s*([^)]+)\)/g, 'await this.tac.__native.share.text($1)')
        .replace(/\bHaptics\.ImpactAsync\(\)/g, 'await this.tac.__native.haptics.impact()')
        .replace(/\bFilePicker\.OpenTextAsync\(\)/g, 'await this.tac.__native.filePicker.openText()')
        .replace(/\bFilePicker\.SaveTextAsync\(/g, 'await this.tac.__native.filePicker.saveText(')
        .replace(/\bSecrets\.GetAsync\(/g, 'await this.tac.__native.secrets.get(')
        .replace(/\bSecrets\.SetAsync\(/g, 'await this.tac.__native.secrets.set(')
        .replace(/\bSecrets\.DeleteAsync\(/g, 'await this.tac.__native.secrets.delete(')
        .replace(/\bAuth\.VerifyUserAsync\(/g, 'await this.tac.__native.auth.verifyUser(')
        .replace(/\bGeolocation\.CurrentAsync\(/g, 'await this.tac.__native.geolocation.current(')
        .replace(/\bNotifications\.ShowAsync\(/g, 'await this.tac.__native.notifications.show(')
        .replace(/\bMedia\.GetUserMediaAsync\(/g, 'await this.tac.__native.media.getUserMedia(')
        .replace(/\bCapabilities\.Supports\(/g, 'this.tac.__native.capabilities.supports(')
        .replace(/\bCapabilities\.StateAsync\(/g, 'await this.tac.__native.capabilities.state(');
}

/**
 * Qualifies companion members without rewriting source text inside strings or
 * comments. Regex-only replacement corrupts URLs such as "/status" when a
 * companion also has a `status` field.
 * @param {string} source
 * @param {string[]} names
 */
function qualifyBareMembers(source, names) {
    const candidates = [...new Set(names)].sort((left, right) => right.length - left.length);
    let output = '';
    let index = 0;
    let quote = '';
    let lineComment = false;
    let blockComment = false;
    while (index < source.length) {
        const character = source[index];
        const next = source[index + 1] ?? '';
        if (lineComment) {
            output += character;
            if (character === '\n') lineComment = false;
            index += 1;
            continue;
        }
        if (blockComment) {
            output += character;
            if (character === '*' && next === '/') {
                output += next;
                index += 2;
                blockComment = false;
            }
            else index += 1;
            continue;
        }
        if (quote) {
            output += character;
            if (character === '\\') {
                output += next;
                index += 2;
                continue;
            }
            if (character === quote) quote = '';
            index += 1;
            continue;
        }
        if (character === '/' && next === '/') {
            output += '//';
            index += 2;
            lineComment = true;
            continue;
        }
        if (character === '/' && next === '*') {
            output += '/*';
            index += 2;
            blockComment = true;
            continue;
        }
        if (character === '"' || character === "'") {
            output += character;
            quote = character;
            index += 1;
            continue;
        }
        const candidate = candidates.find((name) => source.startsWith(name, index));
        const previous = source[index - 1] ?? '';
        const following = source[index + (candidate?.length ?? 0)] ?? '';
        if (candidate && !/[.$\w]/.test(previous) && !/[\w$]/.test(following)) {
            output += `this.${candidate}`;
            index += candidate.length;
            continue;
        }
        output += character;
        index += 1;
    }
    return output;
}

/** @param {string} body @param {TacSubsetLanguage} language @param {string[]} fieldNames @param {string[]} methodNames @param {string} methodName */
function translateBody(body, language, fieldNames, methodNames, methodName) {
    // Strings and comments ride through the lowering as opaque placeholders,
    // so a body may say "navigator.isOnline() failed" without being rewritten.
    const { masked, restore } = extractLiterals(body.trim());
    let output = masked;
    if (/\b(?:import|using|package|require)\b/.test(output))
        throw new Error(`${LANGUAGE_LABELS[language]} Tac companion method '${methodName}' cannot import dependencies.`);
    if (language === 'rust') {
        output = output.replace(/\blet\s+mut\s+/g, 'let ')
            .replace(/\blet\s+([A-Za-z_$][\w$]*)\s*:\s*[^=;\n]+=/g, 'let $1 =')
            .replace(/\bself\./g, 'this.')
            .replace(/\b(if|while)\s+([^\n{]+)\s*\{/g, '$1 ($2) {');
    }
    if (language === 'swift') {
        output = output.replace(/\bself\./g, 'this.')
            .replace(/\b(if|while)\s+([^\n{]+)\s*\{/g, '$1 ($2) {')
            .replace(/\bvar\s+([A-Za-z_$][\w$]*)\s*:\s*[^=;\n]+=/g, 'let $1 =')
            .replace(/\blet\s+([A-Za-z_$][\w$]*)\s*:\s*[^=;\n]+=/g, 'const $1 =');
    }
    if (language === 'kotlin') {
        output = output.replace(/\bvar\s+([A-Za-z_$][\w$]*)\s*:\s*[^=;\n]+=/g, 'let $1 =')
            .replace(/\bval\s+([A-Za-z_$][\w$]*)\s*:\s*[^=;\n]+=/g, 'const $1 =');
    }
    if (language === 'csharp') {
        output = output.replace(/\b(?:int|long|float|double|decimal|bool|string|object)\s+([A-Za-z_$][\w$]*)\s*=/g, 'let $1 =');
    }
    if (language === 'kotlin') {
        output = qualifyBareMembers(output, [...fieldNames, ...methodNames]);
    }
    output = translateNativeShims(output, language);
    // A source language may already spell an asynchronous shim call with
    // `await`; preserve that form when a lowering also needs an await.
    output = output.replace(/\bawait\s+await\s+/g, 'await ');
    return restore(output.replace(/(?<![.$\w])tac\./g, 'this.tac.'));
}

export class TacSubsetCompanionContract {
    /** @param {{ language: TacSubsetLanguage, className: string, fields: TacSubsetField[], methods: TacSubsetMethod[] }} values */
    constructor(values) {
        this.language = values.language;
        this.className = values.className;
        this.fields = values.fields;
        this.methods = values.methods;
    }

    /** @param {string} source @param {TacSubsetLanguage} language @param {string} sourcePath */
    static parse(source, language, sourcePath) {
        const blanked = blankLiterals(source);
        assertNoLegacyPrelude(blanked, language, sourcePath, source);
        const importMatch = /^\s*(?:import|using|package|use)\b/m.exec(blanked);
        if (importMatch)
            throw subsetError(`${LANGUAGE_LABELS[language]} Tac companion must not declare imports. Tac supplies its portable runtime APIs.`, sourcePath, source, importMatch.index);
        assertSupportedConstructs(blanked, language, sourcePath, source);
        if (language === 'rust') {
            const parsed = rustCompanion(source, sourcePath);
            return new TacSubsetCompanionContract({ language, className: parsed.name, fields: parsed.fields, methods: parsed.methods });
        }
        const parsed = companionClass(source, language, sourcePath);
        const context = { sourcePath, source, offset: parsed.bodyStart };
        const methods = methodsFor(parsed.body, language, context);
        return new TacSubsetCompanionContract({
            language,
            className: parsed.name,
            fields: fieldsFor(parsed.body, language, methods, context),
            methods,
        });
    }
}

export default class TacSubsetCompanionCompiler {
    /** @param {TacSubsetLanguage} language */
    constructor(language) {
        this.language = language;
    }

    /** @param {string} source @param {string} sourcePath */
    compile(source, sourcePath) {
        const contract = TacSubsetCompanionContract.parse(source, this.language, sourcePath);
        return { contract, code: this.createJavaScriptController(contract) };
    }

    /** @param {TacSubsetCompanionContract} contract */
    createJavaScriptController(contract) {
        const fieldNames = contract.fields.map((field) => field.name);
        const methodNames = contract.methods.map((method) => method.name);
        const fieldInitializers = contract.fields
            .map((field) => `        this.${field.name} = ${JSON.stringify(field.initial)};`)
            .join('\n');
        const publishedFields = contract.fields.flatMap((field) => {
            const name = annotation(field.annotations, 'publish');
            return name ? [{ field: field.name, name, options: { retain: true } }] : [];
        });
        const subscriptions = contract.methods
            .map((method) => ({ method: method.name, name: annotation(method.annotations, 'subscribe') }))
            .filter((item) => item.name.length > 0);
        const mountMethods = contract.methods
            .filter((method) => method.annotations.some((item) => item.name === 'onmount'))
            .map((method) => method.name);
        const methods = contract.methods.map((method) => {
            const body = translateBody(method.body, contract.language, fieldNames, methodNames, method.name);
            const asyncKeyword = /\bawait\b/.test(blankLiterals(body)) ? 'async ' : '';
            const publishName = annotation(method.annotations, 'publish');
            const parameters = method.parameters.join(', ');
            if (!publishName) return `    ${asyncKeyword}${method.name}(${parameters}) {\n${body}\n    }`;
            const implementationName = `__tc_${method.name}`;
            return `    ${asyncKeyword}${implementationName}(${parameters}) {\n${body}\n    }\n\n    ${method.name}(...args) {\n        const result = this.${implementationName}(...args);\n        if (result && typeof result.then === 'function') {\n            return result.then((value) => {\n                this.tac.publish(${JSON.stringify(publishName)}, value, { retain: true });\n                return value;\n            });\n        }\n        this.tac.publish(${JSON.stringify(publishName)}, result, { retain: true });\n        return result;\n    }`;
        }).join('\n\n');
        return `// Generated by Tachyon's ${LANGUAGE_LABELS[contract.language]} Tac companion frontend.\nexport default class {\n    constructor(props = {}, tac = {}) {\n        this.props = props;\n        this.tac = tac;\n${fieldInitializers}\n        this.__tc_signal_publish_fields__ = ${JSON.stringify(publishedFields)};\n        for (const subscription of ${JSON.stringify(subscriptions)}) {\n            if (typeof this.tac.subscribe === 'function')\n                this.tac.subscribe(subscription.name, (value) => this[subscription.method](value), { immediate: false });\n        }\n        for (const method of ${JSON.stringify(mountMethods)}) {\n            if (typeof this.tac.onMount === 'function')\n                this.tac.onMount(() => this[method]());\n        }\n    }\n\n${methods}\n}\n`;
    }
}
