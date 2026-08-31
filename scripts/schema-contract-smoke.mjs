// Request validation must pass in the downloaded executable, not only unit tests.
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { withServer } from './release/server-probe.mjs';

const binary = process.argv[2] ?? process.env.TAC_BIN ?? resolve('target/release', process.platform === 'win32' ? 'ty.exe' : 'ty');
const root = await mkdtemp(join(tmpdir(), 'tachyon-schema-'));
try {
  await mkdir(join(root, 'client/pages'), { recursive: true });
  const route = join(root, 'server/routes/items/_id');
  await mkdir(route, { recursive: true });
  await writeFile(join(root, 'client/pages/tac.html'), '<main>Schema acceptance</main>');
  await writeFile(join(route, 'yon.js'), '@Controller\nexport class ItemsController { static POST() { return {accepted:true}; } }');
  await writeFile(join(route, 'OPTIONS.schema.json'), JSON.stringify({
    methods: { POST: { request: {
      parameters: { id: '^[0-9]+$' },
      headers: { authorization: '^Bearer .+$' },
      body: { name: '^.{1,10}$' },
    } } },
  }));
  await withServer(binary, ['preview', root, '--host', '127.0.0.1', '--port', '0'], async request => {
    /** @param {string} path @param {string} body */
    const send = (path, body, authorization = 'Bearer fixture-only') => request(path, {
      method: 'POST', headers: { authorization, 'content-type': 'application/json' }, body,
    });
    assert.equal((await send('/items/7', '{"name":"Ada"}')).status, 200);
    assert.equal((await send('/items/7', '{"name":"far too long a name"}')).status, 400);
    assert.equal((await send('/items/nope', '{"name":"Ada"}')).status, 400);
    assert.equal((await send('/items/7', '{"name":"Ada"}', 'invalid')).status, 400);
    assert.equal((await send('/items/7', 'not json')).status, 400);
    const options = await request('/items/7', { method: 'OPTIONS' });
    assert.equal(options.status, 200);
    assert.ok((await options.json()).methods.POST);
    assert.equal((await request('/api.json')).status, 200);
    assert.equal((await request('/api-docs/')).status, 200);
  });
  console.log('PASS: CHEX body/header/parameter validation, dynamic OPTIONS, API reference');
} finally {
  await rm(root, { recursive: true, force: true });
}
