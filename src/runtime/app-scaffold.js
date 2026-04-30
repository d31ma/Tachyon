// @ts-check
import { mkdir, readdir, stat, writeFile } from 'fs/promises';
import path from 'path';
const version = '26.18.30';
const files = {
    '.gitignore': `node_modules
dist
.env
.DS_Store
`,
    '.env.test': `YON_PORT=8000
YON_HOST=127.0.0.1
YON_HOSTNAME=127.0.0.1
YON_DEV=true
YON_LOG_LEVEL=info
YON_LOG_FORMAT=pretty
YON_TRUST_PROXY=
TAC_FORMAT=esm
TAC_PUBLIC_ENV=
YON_MAX_BODY_BYTES=1048576
YON_HANDLER_TIMEOUT_MS=30000
YON_RATE_LIMIT_MAX=
YON_RATE_LIMIT_WINDOW_MS=
YON_HMR_TOKEN=
YON_HMR_MAX_CLIENTS=20
YON_ENABLE_HSTS=false
YON_OTEL_ENABLED=false
YON_OTEL_ROOT=
YON_OTEL_SERVICE_NAME=@d31ma/tachyon
YON_OTEL_SERVICE_VERSION=
YON_OTEL_CAPTURE_IP=false
FYLO_ROOT=db/collections
FYLO_SCHEMA_DIR=db/schemas
# Optional: set to s3-prefix to store FYLO prefix indexes through Bun.S3Client.
# FYLO 26.18.29 uses each collection name directly as its S3 bucket name;
# no bucket-prefix variable is required.
FYLO_INDEX_BACKEND=
FYLO_S3_ACCESS_KEY_ID=
FYLO_S3_SECRET_ACCESS_KEY=
FYLO_S3_SESSION_TOKEN=
FYLO_S3_REGION=
FYLO_S3_ENDPOINT=
FYLO_ENCRYPTION_KEY=
FYLO_CIPHER_SALT=
YON_DATA_BROWSER_ENABLED=false
YON_DATA_BROWSER_READONLY=true
YON_DATA_BROWSER_REVEAL=false
YON_PAGES_PATH=browser/pages
YON_COMPONENTS_PATH=browser/components
YON_ASSETS_PATH=browser/shared/assets
YON_ROUTES_PATH=server/routes
YON_SHARED_SCRIPTS_PATH=browser/shared/scripts
YON_SHARED_STYLES_PATH=browser/shared/styles
YON_SHARED_DATA_PATH=browser/shared/data
`,
    'package.json': JSON.stringify({
        name: 'tachyon-app',
        private: true,
        type: 'module',
        scripts: {
            serve: 'yon.serve',
            bundle: 'tac.bundle',
            preview: 'tac.preview --watch',
            test: 'bun test'
        },
        devDependencies: {
            '@d31ma/tachyon': `^${version}`
        }
    }, null, 2) + '\n',
    'README.md': `# Tachyon App

## Commands

\`\`\`bash
bun install
bun run bundle
bun run preview
bun run serve
\`\`\`

The bundled output is written to \`dist/\`. \`bun run serve\` detects whether the app has \`browser/\`, \`server/\`, or both and serves the matching frontend, backend, or full-stack runtime.
`,
    'browser/shared/scripts/imports.js': `import "../styles/app.css"

document.documentElement.setAttribute('data-theme', 'light')
`,
    'browser/shared/styles/app.css': `:root {
  color-scheme: dark;
}
`,
    'browser/shared/assets/.gitkeep': ``,
    'browser/shared/data/.gitkeep': ``,
    'server/routes/GET.js': `export async function handler() {
  return { ok: true, framework: 'Tachyon' }
}
`,
    'db/schemas/.gitkeep': ``,
    'db/collections/.gitkeep': ``,
    'db/README.md': `# db/

This folder is the default FYLO root for the application.

## Structure

\`\`\`
db/
├── schemas/       # Versioned schemas consumed by FYLO strict validation
└── collections/   # FYLO document store, managed exclusively by @d31ma/fylo
\`\`\`

## schemas/

Place versioned JSON schemas here for FYLO strict validation:

\`\`\`
db/schemas/<collection>/
|-- manifest.json
|-- history/
|   \`-- v1.json
\`-- rules.json        # optional RLS rules
\`\`\`

When schemas declare \`$encrypted\` fields, FYLO will use AES-GCM encryption for
those values. The manifest's \`current\` field selects the head schema version.

## collections/

**Do not modify the contents of this directory by hand.**

Document shards, prefix indexes, event journals, locks, and WORM history are
created and managed exclusively by the \`@d31ma/fylo\` package. Manual edits to
this directory can corrupt storage state and cause data loss.

To rebuild the index from document files:

\`\`\`bash
fylo.admin rebuild <collection> --root db/collections
\`\`\`

## Overriding the root

To use a different FYLO root or schema directory, set:

\`\`\`env
FYLO_ROOT=/path/to/custom/root
FYLO_SCHEMA_DIR=/path/to/custom/schemas
\`\`\`
`,
    'server/data/.gitkeep': ``,
    'server/deps/.gitkeep': ``,
    'browser/pages/index.html': `<div class="shell">
  <div class="brand">
    <strong>Tachyon</strong>
    <nav>
      <a href="/">Home</a>
    </nav>
  </div>

  <slot />

  <hero />
</div>
`,
    'browser/pages/index.js': `document.title = "Tachyon App"
`,
    'browser/pages/index.css': `body { margin: 0; font-family: "IBM Plex Sans", ui-sans-serif, system-ui, sans-serif; background: #0f172a; color: #e2e8f0; }
.shell { max-width: 72rem; margin: 0 auto; padding: 2rem 1.25rem 4rem; }
.brand { display: flex; align-items: center; justify-content: space-between; margin-bottom: 2rem; }
.brand a { color: inherit; text-decoration: none; }
`,
    'browser/components/hero.html': `<section class="hero">
  <h1>Build your next Bun app with Tachyon.</h1>
  <p>File-system routes, reactive Tac pages, static export, and preview tooling are already wired in.</p>
</section>
`,
    'browser/components/hero.css': `.hero { padding: 2rem; border-radius: 1.5rem; background: linear-gradient(135deg, #1d4ed8, #0f766e); }
.hero h1 { margin: 0 0 0.75rem; font-size: clamp(2rem, 6vw, 4rem); }
.hero p { margin: 0; max-width: 42rem; line-height: 1.6; }
`
};
/** @param {string} targetDir */
async function ensureEmptyDirectory(targetDir) {
    try {
        const info = await stat(targetDir);
        if (!info.isDirectory()) {
            throw new Error(`Target '${targetDir}' exists and is not a directory`);
        }
        const entries = await readdir(targetDir);
        if (entries.length > 0) {
            throw new Error(`Target directory '${targetDir}' is not empty`);
        }
    }
    catch (error) {
        if (!(error instanceof Error) || !('code' in error))
            throw error;
        if (error.code !== 'ENOENT')
            throw error;
        await mkdir(targetDir, { recursive: true });
    }
}
/** @param {string} targetDir */
export async function createAppScaffold(targetDir) {
    const resolved = path.resolve(targetDir);
    await ensureEmptyDirectory(resolved);
    for (const [relativePath, contents] of Object.entries(files)) {
        const outputPath = path.join(resolved, relativePath);
        await mkdir(path.dirname(outputPath), { recursive: true });
        await writeFile(outputPath, contents);
    }
    return resolved;
}
