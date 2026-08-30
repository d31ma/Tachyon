// Configuration is a module rather than a data file, so it can derive what a
// static document could only repeat.
import { readFile, writeFile } from 'node:fs/promises'

const ORIGIN = process.env.TAC_SITE_ORIGIN ?? 'https://tachyon.del.ma'

/**
 * What each route puts in the document head.
 *
 * A `tac.html` is a view rather than a page of HTML, so this is where a
 * route's title, description and social tags live. Derived from ORIGIN so a
 * channel build does not advertise the production canonical.
 */
const metadata = {
  "/": {
    title: "Tachyon — one project, web and native",
    description: "A polyglot, file-system-routed full-stack framework. Author HTML once; ship it to the browser and to every desktop and mobile target.",
    canonical: `${ORIGIN}/`,
    image: `${ORIGIN}/shared/assets/logo.svg`,
    siteName: "Tachyon",
  },
  "/docs": {
    title: "Documentation — Tachyon",
    description: "Guides for Tac views, Yon endpoints, supported languages and build targets, and every feature with the code that proves it.",
    canonical: `${ORIGIN}/docs`,
    image: `${ORIGIN}/shared/assets/logo.svg`,
    siteName: "Tachyon",
  },
  "/docs/_topic": {
    title: "Documentation — Tachyon",
    description: "Tachyon guide.",
    canonical: `${ORIGIN}/docs`,
    image: `${ORIGIN}/shared/assets/logo.svg`,
    siteName: "Tachyon",
    noindex: true,
  },
  "/docs/features": {
    title: "Features — Tachyon",
    description: "Every feature Tachyon has, each on its own page with the code that proves it — routing, views, storage, the server, native targets and the toolchain.",
    canonical: `${ORIGIN}/docs/features`,
    image: `${ORIGIN}/shared/assets/logo.svg`,
    siteName: "Tachyon",
  },
  "/docs/features/_id": {
    title: "Features — Tachyon",
    description: "One Tachyon feature, with the code that proves it.",
    canonical: `${ORIGIN}/docs/features`,
    image: `${ORIGIN}/shared/assets/logo.svg`,
    siteName: "Tachyon",
    noindex: true,
  },
}

const attribute = (value) => String(value)
  .replaceAll('&', '&amp;')
  .replaceAll('<', '&lt;')
  .replaceAll('>', '&gt;')
  .replaceAll('"', '&quot;')

function metadataHead(meta) {
  const tags = [
    `<title>${attribute(meta.title)}</title>`,
    meta.noindex
      ? '<meta name="robots" content="noindex, follow">'
      : '<meta name="robots" content="index, follow, max-image-preview:large">',
    `<meta name="description" content="${attribute(meta.description)}">`,
    `<link rel="canonical" href="${attribute(meta.canonical)}">`,
    '<link rel="manifest" href="/manifest.webmanifest">',
    '<meta name="theme-color" content="#07090d">',
  ]
  if (!meta.noindex) {
    tags.push(
      '<meta property="og:type" content="website">',
      `<meta property="og:site_name" content="${attribute(meta.siteName)}">`,
      `<meta property="og:title" content="${attribute(meta.title)}">`,
      `<meta property="og:description" content="${attribute(meta.description)}">`,
      `<meta property="og:url" content="${attribute(meta.canonical)}">`,
      `<meta property="og:image" content="${attribute(meta.image)}">`,
      '<meta name="twitter:card" content="summary_large_image">',
      `<meta name="twitter:title" content="${attribute(meta.title)}">`,
      `<meta name="twitter:description" content="${attribute(meta.description)}">`,
      `<meta name="twitter:image" content="${attribute(meta.image)}">`,
    )
  }
  return tags.join('')
}

async function applyMetadata(root, route, meta) {
  const relative = route === '/' ? 'index.html' : `${route.slice(1)}/index.html`
  const file = `${root}/${relative}`
  let html = await readFile(file, 'utf8')
  if (html.includes('<!--site-metadata-->')) return
  html = html.replace('<html>', '<html lang="en">')
  html = html.replace('</head>', `${metadataHead(meta)}<!--site-metadata--></head>`)
  await writeFile(file, html)
}

/**
 * Emits the discovery files a documentation site is judged on.
 *
 * They are generated rather than authored because the route list is already
 * known: the compiler wrote a manifest, and the guides are data. Writing them
 * by hand is how a sitemap ends up describing a site that no longer exists.
 */
export async function postBundle({ targetRoots }) {
  const root = targetRoots.web
  if (!root) return

  const docs = JSON.parse(await readFile(new URL('./client/shared/data/docs.json', import.meta.url), 'utf8'))
  const catalogue = JSON.parse(await readFile(new URL('./client/shared/data/features.json', import.meta.url), 'utf8'))
  const manifest = JSON.parse(await readFile(`${root}/route-manifest.json`, 'utf8'))

  for (const [route, meta] of Object.entries(metadata)) {
    await applyMetadata(root, route, meta)
  }
  await writeFile(
    `${root}/manifest.webmanifest`,
    await readFile(new URL('./client/shared/assets/manifest.webmanifest', import.meta.url)),
  )

  // A dynamic route is a pattern, not a page, so it is replaced by the topics
  // it actually serves.
  const paths = new Set(['/'])
  for (const route of manifest.routes ?? []) {
    const path = route.route ?? route.path
    if (typeof path !== 'string' || path.includes('_')) continue
    if (path.startsWith('/api')) continue
    paths.add(path)
  }
  for (const slug of docs.order) paths.add(`/docs/${slug}`)
  for (const feature of catalogue.features) paths.add(`/docs/features/${feature.id}`)

  const urls = [...paths].sort().map((path) =>
    `  <url><loc>${ORIGIN}${path === '/' ? '/' : path}</loc></url>`).join('\n')
  await writeFile(`${root}/sitemap.xml`,
    `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`)

  // The API routes answer for the application, not for a crawler.
  await writeFile(`${root}/robots.txt`,
    `User-agent: *\nAllow: /\nDisallow: /api/\n\nSitemap: ${ORIGIN}/sitemap.xml\n`)
}
