# db/

This folder is the database layer for the example app.

## Layout

```text
db/
|-- schemas/       # Versioned FYLO/CHEX schemas, developer-owned
|-- seed/          # Human-editable seed documents, developer-owned
`-- collections/   # FYLO-managed storage, do not hand-edit
```

## schemas/

Each collection uses FYLO's versioned schema layout:

```text
db/schemas/<collection>/
|-- manifest.json
`-- history/
    `-- v1.json
```

The files in `history/` are CHEX regex schemas. Each leaf schema value is a regex string, and the JSON filename under `seed/` is the document id, so schemas should not include redundant `id`, `createdAt`, or `updatedAt` fields.

## seed/

Seed files live at:

```text
db/seed/<collection>/<document-id>.json
```

Run `bun run seed` from `examples/` to validate and import these documents into FYLO.

## collections/

`db/collections/` is owned by FYLO. Do not modify files under `.fylo/` by hand. If collection state drifts, rebuild indexes from the managed docs with:

```bash
fylo.admin rebuild <collection> --root db/collections
```
