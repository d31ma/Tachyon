#!/usr/bin/env bun
// @ts-check

// One-shot seeder. Reads JSON files under db/seed/<collection>/*.json and
// writes them into the FYLO-managed root.
//
// Each filename (minus the .json suffix) IS the TTID of the resulting document
// — preserved via fylo.executePutDataDirect() rather than fylo.putData(),
// which would discard the supplied id and assign a fresh one.
//
// db/collections/ is owned by FYLO. Don't hand-edit files there. Put seed
// data in db/seed/ and run: bun run seed

import path from 'path'
import Fylo from '@d31ma/fylo'
import { fyloOptions } from '../../src/server/fylo-options.js'

const seedRoot = path.join(process.cwd(), 'db/seed')
const fyloRoot = process.env.FYLO_ROOT || path.join(process.cwd(), 'db/collections')
const schemaRoot = process.env.FYLO_SCHEMA_DIR || path.join(process.cwd(), 'db/schemas')

process.env.FYLO_SCHEMA_DIR = schemaRoot

const fylo = new Fylo(fyloOptions(fyloRoot))

const collections = await Array.fromAsync(new Bun.Glob('*/').scan({ cwd: seedRoot, onlyFiles: false }))
const summary = []

for (const dir of collections) {
    const collection = dir.replace(/\/$/, '')
    if (!collection) continue

    try {
        await fylo.createCollection(collection)
    } catch {
        // already exists
    }

    const files = await Array.fromAsync(new Bun.Glob('*.json').scan({ cwd: path.join(seedRoot, collection) }))

    let imported = 0
    let skipped = 0
    for (const file of files) {
        const filePath = path.join(seedRoot, collection, file)
        const seedId = file.replace(/\.json$/, '')
        try {
            const doc = await Bun.file(filePath).json()
            // prepareInsert applies schema-driven encryption + resolves WORM
            // previousId; we then override the generated _id with the filename
            // TTID so the seed identity is stable across imports.
            const prepared = await fylo.prepareInsert(collection, doc)
            await fylo.executePutDataDirect(collection, seedId, prepared.doc, prepared.previousId)
            imported++
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error)
            console.error(`  [${collection}] ${file}: ${message}`)
            skipped++
        }
    }

    summary.push({ collection, imported, skipped, total: files.length })
}

console.log('\nSeed complete:')
for (const row of summary) {
    console.log(`  ${row.collection}: ${row.imported}/${row.total} imported${row.skipped ? ` (${row.skipped} skipped)` : ''}`)
}
console.log(`\nFYLO root: ${fyloRoot}`)
console.log(`FYLO schema root: ${schemaRoot}`)
