// @ts-check
import { mkdir, readdir, stat, writeFile } from 'fs/promises';
import path from 'path';
const version = '2.0.0';
const files = {
    '.gitignore': `node_modules
dist
.env
.DS_Store
`,
    '.env.example': `PORT=8000
HOST=127.0.0.1
HOSTNAME=127.0.0.1
DEV=true
LOG_LEVEL=info
LOG_FORMAT=pretty
TRUST_PROXY=
TAC_FORMAT=esm
TAC_PUBLIC_ENV=
MAX_BODY_BYTES=1048576
HANDLER_TIMEOUT_MS=30000
RATE_LIMIT_MAX=
RATE_LIMIT_WINDOW_MS=
HMR_TOKEN=
HMR_MAX_CLIENTS=20
ENABLE_HSTS=false
OTEL_ENABLED=false
OTEL_FYLO_ROOT=
OTEL_SERVICE_NAME=@d31ma/tachyon
OTEL_SERVICE_VERSION=
OTEL_CAPTURE_IP=false
PAGES_PATH=browser/pages
COMPONENTS_PATH=browser/components
ASSETS_PATH=browser/shared/assets
ROUTES_PATH=server/routes
SHARED_SCRIPTS_PATH=browser/shared/scripts
SHARED_STYLES_PATH=browser/shared/styles
SHARED_DATA_PATH=browser/shared/data
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
    'browser/shared/scripts/main.js': `import "../styles/app.css"

document.documentElement.setAttribute('data-theme', 'light')
`,
    'browser/shared/styles/app.css': `:root {
  color-scheme: dark;
}
`,
    'browser/shared/assets/.gitkeep': ``,
    'browser/shared/data/.gitkeep': ``,
    'server/routes/GET': `#!/usr/bin/env bun

Bun.stdout.write(JSON.stringify({ ok: true, framework: 'Tachyon' }))
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
