# db/

This folder is the database layer for the example app.

## Layout

```text
db/
|-- schemas/       # Versioned FYLO/CHEX schemas, developer-owned
|-- .collections/  # Committed example FYLO storage
`-- .queue/        # Runtime-created durable Yon realtime message queues
```

## schemas/

Each collection uses FYLO's versioned schema layout:

```text
db/schemas/<collection>/
|-- manifest.json
`-- history/
    `-- v1.schema.json
```

The files in `history/` are CHEX regex schemas. Each leaf schema value is a regex string, and FYLO stores document identity in the collection path, so schemas should not include redundant `id`, `createdAt`, or `updatedAt` fields.

## .collections/

`db/` is the example app's FYLO root (`FYLO_ROOT=db`). The checked-in data intentionally uses FYLO's production layout:

```text
db/.collections/<collection>/docs/<prefix>/<document-id>.json
```

Do not place bare JSON files directly under a collection directory. FYLO owns each collection's `docs/`, `index/`, `events/`, and `locks/` folders. If collection state drifts, rebuild indexes from the managed docs with:

```bash
fylo.admin rebuild <collection> --root db
```

## .queue/

The realtime messaging example stores per-client mailbox events through FYLO's
`LocalQueue` under `db/.queue/`. Browser clients keep an SSE connection open to
`/_yon/realtime/stream`; if the server restarts or a client reconnects later,
Yon replays messages after the client's cursor from this durable queue.
