// @ts-check

export default class {
  serveCommand = [
    'ty init api --name "My API"',
    'cd api',
    'mkdir -p server/routes/health',
    'ty serve',
  ].join('\n')

  capabilities = [
    {
      title: 'Shape-aware serving',
      subtitle: 'client, server, or full stack',
      body: 'ty serve detects whether the app has client/, server/, db/, or a mix, then serves the right runtime on one port.',
      code: 'ty serve\nYON_PORT=8000 ty serve\nNODE_ENV=production ty serve',
    },
    {
      title: 'Route handlers',
      subtitle: 'FastAPI-style returns',
      body: 'Handlers return plain data. Yon serializes, infers content type, applies headers, validates the contract and chooses the matching status schema.',
      code: 'export class Handler {\n  static GET() { return { ok: true } }\n  static POST(request) { return request.body }\n}',
    },
    {
      title: 'Contract files',
      subtitle: 'CHEX + OpenAPI',
      body: 'OPTIONS.schema.json validates requests and responses and feeds the generated /openapi.json plus self-hosted /api-docs UI.',
      code: 'server/routes/items/\n  yon.ts\n  OPTIONS.schema.json',
    },
    {
      title: 'Durable realtime',
      subtitle: 'SSE mailboxes',
      body: 'YON_REALTIME_ENABLED mounts TTID client registration, SSE streams and durable FYLO-backed message queues under /_yon/realtime.',
      code: 'YON_REALTIME_ENABLED=true ty serve',
    },
    {
      title: 'FYLO browser API',
      subtitle: '/_fylo',
      body: 'Enable the built-in FYLO browser and Django/PostgREST-style collection endpoints for local data inspection and controlled mutation.',
      code: 'YON_DATA_BROWSER_ENABLED=true\nYON_DATA_BROWSER_READONLY=false',
    },
    {
      title: 'Polyglot middleware',
      subtitle: 'before, after, rateLimit',
      body: 'Keep middleware.js for JavaScript, or write server/middleware/yon.<ext>. Class-style middleware works for JS, TS, Python, Ruby and PHP; every other language can use the raw JSON shim protocol.',
      code: 'server/middleware/yon.py\nclass Middleware:\n  @staticmethod\n  def before(input): ...',
    },
    {
      title: 'Operations',
      subtitle: 'security and telemetry',
      body: 'Yon wraps filesystem and inline routes with CORS rejection, Basic Auth, request limits, middleware, request IDs, trace headers and OTLP JSON spans persisted into FYLO.',
      code: 'YON_VALIDATE=true\nYON_BASIC_AUTH_HASH=...\nYON_OTEL_ENABLED=true',
    },
  ]

  languages = [
    'JavaScript',
    'TypeScript',
    'Python',
    'Ruby',
    'PHP',
    'Dart',
    'Java',
    'C#',
    'C++',
    'Rust',
  ]
}
