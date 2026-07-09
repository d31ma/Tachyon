// @ts-check
// Enterprise SEO for the Tachyon site, applied as a post-bundle pass over the
// prerendered HTML. Because every route is prerendered to static HTML, crawlers
// receive fully-formed <head> metadata on first fetch — no JS execution needed.
//
// This runs after every build (both `bundle` and `serve`), so it is the single
// source of truth for titles, descriptions, canonical URLs, Open Graph / Twitter
// cards and JSON-LD. It also emits robots.txt and sitemap.xml.
//
// ⚠️ Set SITE_URL to the real production origin — a wrong canonical/OG URL is
//    worse for SEO than none. Everything below derives from it.

import { readFile, writeFile } from 'fs/promises';
import path from 'path';

const SITE_URL = 'https://tachyon.dev';        // production origin, no trailing slash
const SITE_NAME = 'Tachyon';
const TWITTER = '@tachyon';                     // brand handle (verify)
const OG_IMAGE = `${SITE_URL}/shared/assets/logo.svg`; // TODO: prefer a 1200×630 PNG for richer cards

/**
 * Per-route SEO. `title` overrides the page's <title>; `description` drives the
 * meta description and social cards. `noindex` keeps a route out of the index and
 * the sitemap (used for the dynamic docs template, which has no canonical URL).
 * @type {Record<string, { file: string, title?: string, description?: string, noindex?: boolean }>}
 */
const ROUTES = {
    '/': {
        file: 'index.html',
        title: 'Tachyon — the polyglot full-stack framework for ty',
        description: 'Tachyon is a polyglot, file-system-routed full-stack framework distributed through the standalone ty binary. Render reactive pages and components with Tac, serve routes in JavaScript, TypeScript, Python, Rust and more with Yon, and persist documents, realtime mailboxes and telemetry with FYLO.',
    },
    '/atlas': {
        file: 'atlas/index.html',
        title: 'Capability atlas — Tachyon',
        description: "A living tour of Tachyon's reactive interfaces, native web capabilities, polyglot Wasm workers, durable browser data and observable client flows — running entirely in the browser with no server behind it.",
    },
    '/docs': {
        file: 'docs/index.html',
        title: 'Documentation — Tachyon',
        description: 'Guides and reference for building full-stack apps with Tachyon: file-system routing, polyglot backends, the Tac rendering model, components, and FYLO storage.',
    },
    '/docs/_topic': {
        file: 'docs/_topic/index.html',
        noindex: true, // dynamic template — real topics are client-rendered, not distinct URLs
    },
};

/** @param {string} value */
const attr = (value) => String(value)
    .replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');

/** JSON-LD is embedded in a <script>; escape `<` so a value can't break out of the tag. */
const jsonLd = (data) => JSON.stringify(data).replaceAll('<', '\\u003c');

/** Site-wide structured data (brand entity + site), emitted once on the homepage. */
const organizationLd = () => jsonLd({
    '@context': 'https://schema.org',
    '@graph': [
        {
            '@type': 'Organization',
            '@id': `${SITE_URL}/#organization`,
            name: SITE_NAME,
            url: SITE_URL,
            logo: `${SITE_URL}/shared/assets/logo.svg`,
            sameAs: ['https://github.com/d31ma/Tachyon'],
        },
        {
            '@type': 'WebSite',
            '@id': `${SITE_URL}/#website`,
            name: SITE_NAME,
            url: SITE_URL,
            publisher: { '@id': `${SITE_URL}/#organization` },
        },
    ],
});

/**
 * @param {string} route
 * @param {{ title?: string, description?: string, noindex?: boolean }} meta
 * @returns {string} the <head> block to inject
 */
function headFor(route, meta) {
    const canonical = `${SITE_URL}${route}`;
    const title = meta.title ?? SITE_NAME;
    const description = meta.description ?? '';
    const tags = [
        meta.noindex
            ? '<meta name="robots" content="noindex, follow">'
            : '<meta name="robots" content="index, follow, max-image-preview:large">',
        `<link rel="canonical" href="${attr(canonical)}">`,
    ];
    if (description)
        tags.push(`<meta name="description" content="${attr(description)}">`);
    if (!meta.noindex) {
        tags.push(
            '<meta property="og:type" content="website">',
            `<meta property="og:site_name" content="${attr(SITE_NAME)}">`,
            `<meta property="og:title" content="${attr(title)}">`,
            description && `<meta property="og:description" content="${attr(description)}">`,
            `<meta property="og:url" content="${attr(canonical)}">`,
            `<meta property="og:image" content="${attr(OG_IMAGE)}">`,
            '<meta name="twitter:card" content="summary_large_image">',
            `<meta name="twitter:site" content="${attr(TWITTER)}">`,
            `<meta name="twitter:title" content="${attr(title)}">`,
            description && `<meta name="twitter:description" content="${attr(description)}">`,
            `<meta name="twitter:image" content="${attr(OG_IMAGE)}">`,
        );
        if (route === '/')
            tags.push(`<script type="application/ld+json">${organizationLd()}</script>`);
    }
    return tags.filter(Boolean).map((tag) => `    ${tag}`).join('\n');
}

/** Inject the SEO <head> block (and force the title) into one prerendered file. */
async function applySeo(webRoot, route, meta) {
    const file = path.join(webRoot, meta.file);
    let html = await readFile(file, 'utf8').catch(() => null);
    if (html == null || html.includes('<!--seo-->'))
        return false;
    if (meta.title)
        html = html.replace(/<title>[\s\S]*?<\/title>/, `<title>${attr(meta.title)}</title>`);
    html = html.replace('</head>', `${headFor(route, meta)}\n    <!--seo-->\n</head>`);
    await writeFile(file, html);
    return true;
}

function robotsTxt() {
    return `User-agent: *\nAllow: /\n\nSitemap: ${SITE_URL}/sitemap.xml\n`;
}

function sitemapXml() {
    const today = new Date().toISOString().slice(0, 10);
    const urls = Object.entries(ROUTES)
        .filter(([, meta]) => !meta.noindex)
        .map(([route]) => `  <url>\n    <loc>${SITE_URL}${route}</loc>\n    <lastmod>${today}</lastmod>\n    <changefreq>weekly</changefreq>\n    <priority>${route === '/' ? '1.0' : '0.8'}</priority>\n  </url>`)
        .join('\n');
    return `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`;
}

/** @param {{ targetRoots: Record<string, string> }} context */
export async function postBundle({ targetRoots }) {
    const webRoot = targetRoots.web;
    if (!webRoot)
        return; // SEO artifacts are web-only
    for (const [route, meta] of Object.entries(ROUTES))
        await applySeo(webRoot, route, meta);
    await writeFile(path.join(webRoot, 'robots.txt'), robotsTxt());
    await writeFile(path.join(webRoot, 'sitemap.xml'), sitemapXml());
}

export default { postBundle };
