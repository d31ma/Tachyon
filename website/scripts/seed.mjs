#!/usr/bin/env node
// Seeds the site's Fylo store.
//
// The docs themselves stay authored as data in the repository: the store holds
// the search index derived from them. Reseeding is idempotent, so this is safe
// to run on every deploy.

import { Fylo } from '../server/vendor/fylo.mjs'
import { readFile } from 'node:fs/promises'
import process from 'node:process'

const ROOT = process.env.TAC_SITE_STORE ?? '/Volumes/DATA/tachyon-site'
const here = (relative) => new URL(relative, import.meta.url)

const docs = JSON.parse(await readFile(here('../client/shared/data/docs.json'), 'utf8'))
const catalogue = JSON.parse(await readFile(here('../client/shared/data/features.json'), 'utf8'))

/**
 * One row per section, so a hit can link straight to the heading.
 *
 * Features are indexed too, now that each is a page rather than a selection in
 * an explorer: before, a search could only ever land the reader on /docs and
 * leave them to find the one they had searched for.
 */
const searchRows = () => {
  const rows = []
  for (const slug of docs.order) {
    const topic = docs.topics[slug]
    rows.push({
      kind: 'topic', slug, heading: topic.title, path: `/docs/${slug}`,
      title: topic.title, text: topic.summary,
    })
    for (const section of topic.sections ?? []) {
      rows.push({
        kind: 'section', slug, heading: section.heading,
        path: `/docs/${slug}#${section.heading.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`,
        title: `${topic.title} · ${section.heading}`,
        text: [section.body, section.code, ...(section.files ?? []).map((f) => f.code)]
          .filter(Boolean).join('\n'),
      })
    }
  }
  for (const feature of catalogue.features) {
    rows.push({
      kind: 'feature', slug: feature.id, heading: feature.title,
      path: `/docs/features/${feature.id}`,
      title: feature.title,
      // The code is indexed with the prose: a reader searching for `@subscribe`
      // or `tacPublish` is searching for the snippet, not for the sentence
      // above it.
      text: [feature.summary, feature.group, ...(feature.files ?? []).map((f) => f.code)]
        .filter(Boolean).join('\n'),
    })
  }
  return rows
}

const db = new Fylo(ROOT)
try {
  for (const [name, rows] of [['search', searchRows()]]) {
    // Dropping first keeps a reseed from stacking duplicates on top of the old
    // rows, which is what makes this safe to run repeatedly.
    try { await db.dropCollection(name) } catch { /* first run */ }
    await db.createCollection(name)
    for (const row of rows) await db.putData(name, row)
    console.log(`${name}: ${rows.length}`)
  }
} finally {
  await db.close()
}
