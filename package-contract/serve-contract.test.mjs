import { afterEach, expect, test } from 'bun:test'
import { createTachyonApp, getFreePort, startTachyon, waitFor } from './helpers.mjs'

const apps = []
const processes = []

afterEach(async () => {
  for (const proc of processes.splice(0)) {
    proc.kill()
    await proc.exited.catch(() => {})
  }

  await Promise.all(apps.splice(0).map((app) => app.cleanup()))
})

test('yon.serve handles API requests with request ids and JSON bodies', async () => {
  const app = await createTachyonApp()
  apps.push(app)

  await app.writeFile('server/routes/api/POST.js', [
    'export function handler(input) {',
    '  return {',
    '    ok: true,',
    '    body: input.body,',
    '    requestId: input.context.requestId,',
    '    query: input.query,',
    '  }',
    '}',
  ].join('\n'))

  const port = await getFreePort()
  const proc = startTachyon(app, {
    env: {
      YON_PORT: String(port),
      YON_HOST: '127.0.0.1',
      YON_LOG_FORMAT: 'json',
    },
  })
  processes.push(proc)

  const baseUrl = `http://127.0.0.1:${port}`
  const ready = await waitFor(async () => {
    try {
      const res = await fetch(`${baseUrl}/health`)
      return res.ok
    } catch {
      return false
    }
  })
  expect(ready).toBe(true)

  const res = await fetch(`${baseUrl}/api?count=2&enabled=true`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Request-Id': 'package-contract-request-id',
    },
    body: JSON.stringify({ name: 'Tachyon' }),
  })
  const body = await res.json()

  expect(res.status).toBe(200)
  expect(res.headers.get('x-request-id')).toBe('package-contract-request-id')
  expect(body.ok).toBe(true)
  expect(body.requestId).toBe('package-contract-request-id')
  expect(body.body.name).toBe('Tachyon')
  expect(body.query.count).toBe(2)
  expect(body.query.enabled).toBe(true)
})

test('production HTML fallback does not inject the development HMR client', async () => {
  const app = await createTachyonApp()
  apps.push(app)

  await app.writeFile('browser/pages/index.html', '<main>Production shell</main>')

  const port = await getFreePort()
  const proc = startTachyon(app, {
    env: {
      YON_PORT: String(port),
      YON_HOST: '127.0.0.1',
      NODE_ENV: 'production',
    },
  })
  processes.push(proc)

  const baseUrl = `http://127.0.0.1:${port}`
  const ready = await waitFor(async () => {
    try {
      const res = await fetch(`${baseUrl}/`, { headers: { Accept: 'text/html' } })
      return res.ok
    } catch {
      return false
    }
  })
  expect(ready).toBe(true)

  const htmlRes = await fetch(`${baseUrl}/`, { headers: { Accept: 'text/html' } })
  const html = await htmlRes.text()
  const clientRes = await fetch(`${baseUrl}/hot-reload-client.js`)
  const clientCode = await clientRes.text()

  expect(htmlRes.status).toBe(200)
  expect(html).toContain('/spa-renderer.js')
  expect(html).not.toContain('/hot-reload-client.js')
  expect(clientRes.status).toBe(200)
  expect(clientCode).toContain('.ok')
  expect(clientCode).toContain('event:')
  expect(clientCode).toContain('reload')
})
