import { mkdir, readdir, stat, writeFile } from 'node:fs/promises'
import path from 'node:path'

const version = '1.10.0'

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
YON_FORMAT=esm
MAX_BODY_BYTES=1048576
HMR_TOKEN=
HMR_MAX_CLIENTS=20
ENABLE_HSTS=false
`,
    'package.json': JSON.stringify({
        name: 'tachyon-app',
        private: true,
        type: 'module',
        scripts: {
            serve: 'tach.serve',
            bundle: 'tach.bundle',
            preview: 'tach.preview --watch',
            test: 'bun test'
        },
        devDependencies: {
            '@delma/tachyon': `^${version}`
        }
    }, null, 2) + '\n',
    'README.md': `# Tachyon App

## Commands

\`\`\`bash
bun install
bun run bundle
bun run preview
bun run serve
bun run serve --full
\`\`\`

The bundled output is written to \`dist/\`. \`bun run serve --full\` serves the frontend and backend on the same port.
`,
    'amplify.yml': `version: 1
frontend:
  phases:
    preBuild:
      commands:
        - curl -fsSL https://bun.sh/install | bash
        - export PATH="$HOME/.bun/bin:$PATH"
        - bun install --frozen-lockfile
    build:
      commands:
        - export PATH="$HOME/.bun/bin:$PATH"
        - bun run bundle
  artifacts:
    baseDirectory: dist
    files:
      - '**/*'
`,
    'main.js': `document.documentElement.setAttribute('data-theme', 'light')
`,
    'routes/GET': `#!/usr/bin/env bun

Bun.stdout.write(JSON.stringify({ ok: true, framework: 'Tachyon' }))
`,
    'routes/LAYOUT': `<style>
  body { margin: 0; font-family: Inter, ui-sans-serif, system-ui, sans-serif; background: #0f172a; color: #e2e8f0; }
  .shell { max-width: 72rem; margin: 0 auto; padding: 2rem 1.25rem 4rem; }
  .brand { display: flex; align-items: center; justify-content: space-between; margin-bottom: 2rem; }
  .brand a { color: inherit; text-decoration: none; }
</style>

<div class="shell">
  <div class="brand">
    <strong>Tachyon</strong>
    <nav>
      <a href="/">Home</a>
    </nav>
  </div>
  <slot />
</div>
`,
    'routes/HTML': `<script>
  document.title = "Tachyon App"
</script>

<hero />
`,
    'components/hero.html': `<style>
  .hero { padding: 2rem; border-radius: 1.5rem; background: linear-gradient(135deg, #1d4ed8, #0f766e); }
  .hero h1 { margin: 0 0 0.75rem; font-size: clamp(2rem, 6vw, 4rem); }
  .hero p { margin: 0; max-width: 42rem; line-height: 1.6; }
</style>

<section class="hero">
  <h1>Build your next Bun app with Tachyon.</h1>
  <p>File-system routes, reactive Yon pages, static export, and preview tooling are already wired in.</p>
</section>
`
} as const

async function ensureEmptyDirectory(targetDir: string) {
    try {
        const info = await stat(targetDir)
        if (!info.isDirectory()) {
            throw new Error(`Target '${targetDir}' exists and is not a directory`)
        }
        const entries = await readdir(targetDir)
        if (entries.length > 0) {
            throw new Error(`Target directory '${targetDir}' is not empty`)
        }
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error
        await mkdir(targetDir, { recursive: true })
    }
}

export async function createAppScaffold(targetDir: string) {
    const resolved = path.resolve(targetDir)
    await ensureEmptyDirectory(resolved)

    for (const [relativePath, contents] of Object.entries(files)) {
        const outputPath = path.join(resolved, relativePath)
        await mkdir(path.dirname(outputPath), { recursive: true })
        await writeFile(outputPath, contents)
    }

    return resolved
}
