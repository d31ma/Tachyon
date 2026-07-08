// @ts-check
// Guards the SEO post-bundle pass (tac.config.js). Reads the built output, so it
// assumes a bundle has run (as the other site tests do).
import { describe, expect, test } from 'bun:test';
import { readFile } from 'fs/promises';

const read = (p) => readFile(new URL(`../${p}`, import.meta.url), 'utf8');

describe('enterprise SEO is baked into the prerendered output', () => {
    test('the homepage head carries description, canonical, OG, Twitter and JSON-LD', async () => {
        const html = await read('dist/web/index.html');
        expect(html).toContain('<meta name="description" content="Tachyon is a polyglot');
        expect(html).toContain('<link rel="canonical" href="https://tachyon.dev/">');
        expect(html).toContain('property="og:title"');
        expect(html).toContain('name="twitter:card" content="summary_large_image"');
        expect(html).toContain('"@type":"Organization"');
        expect(html).toContain('content="index, follow');
    });

    test('each route gets a distinct title, description and canonical', async () => {
        const [home, atlas, docs] = await Promise.all([
            read('dist/web/index.html'),
            read('dist/web/atlas/index.html'),
            read('dist/web/docs/index.html'),
        ]);
        const canonical = (h) => (h.match(/rel="canonical" href="([^"]+)"/) || [])[1];
        expect(canonical(home)).toBe('https://tachyon.dev/');
        expect(canonical(atlas)).toBe('https://tachyon.dev/atlas');
        expect(canonical(docs)).toBe('https://tachyon.dev/docs');
        // Descriptions differ per page (no copy-paste boilerplate).
        const desc = (h) => (h.match(/name="description" content="([^"]+)"/) || [])[1];
        expect(new Set([desc(home), desc(atlas), desc(docs)]).size).toBe(3);
    });

    test('the dynamic docs template is noindex and out of the sitemap', async () => {
        const topic = await read('dist/web/docs/_topic/index.html');
        expect(topic).toContain('content="noindex, follow"');
        expect(topic).not.toContain('property="og:title"'); // no social card for a non-URL
        const sitemap = await read('dist/web/sitemap.xml');
        expect(sitemap).not.toContain('_topic');
    });

    test('robots.txt allows crawling and points at the sitemap', async () => {
        const robots = await read('dist/web/robots.txt');
        expect(robots).toContain('User-agent: *');
        expect(robots).toContain('Sitemap: https://tachyon.dev/sitemap.xml');
    });

    test('sitemap.xml lists exactly the three indexable routes', async () => {
        const sitemap = await read('dist/web/sitemap.xml');
        const locs = [...sitemap.matchAll(/<loc>([^<]+)<\/loc>/g)].map((m) => m[1]);
        expect(locs).toEqual([
            'https://tachyon.dev/',
            'https://tachyon.dev/atlas',
            'https://tachyon.dev/docs',
        ]);
    });
});
