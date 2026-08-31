// Run against the artifact selected by TAC_BIN, without rebuilding Tachyon.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { withServer } from './server-probe.mjs';

const binary = process.env.TAC_BIN ?? resolve('target/release', process.platform === 'win32' ? 'ty.exe' : 'ty');
const root = await mkdtemp(join(tmpdir(), 'tachyon-feature-'));
try {
  const pages = join(root, 'client/pages');
  await mkdir(pages, { recursive: true });
  await writeFile(join(pages, 'tac.html'), '\uFEFF<main title="&amp;lt;"><loop :for="let i = 3; i >= 0; i--"><p>{i}</p></loop></main>');
  const result = spawnSync(binary, ['bundle', root], { encoding: 'utf8', timeout: 30_000, maxBuffer: 1024 * 1024 });
  assert.equal(result.status, 0, `${result.error ?? result.stderr}`);
  const output = join(root, 'dist/web/index.html');
  const published = await readFile(output, 'utf8');
  const encoded = published.split('data-tachyon-runtime>')[1]?.split('</script>')[0];
  assert.ok(encoded, 'client render plan is missing');
  const plan = JSON.parse(encoded);
  assert.equal(plan.route, '/');
  assert.equal(plan.nodes[0].attributes[0].value, '&lt;', 'entities must decode only once');
  assert.equal(plan.nodes[0].children[0].k, 'counted');
  assert.equal(plan.nodes[0].children[0].comparison, 'ge');
  assert.ok(published.includes('name="viewport"'));
  await writeFile(join(pages, 'tac.html'), '<main>Unpublished change</main>');
  await withServer(binary, ['start', root, '--host', '127.0.0.1', '--port', '0'], async request => {
    const response = await request('/');
    assert.equal(response.status, 200);
    assert.equal(await response.text(), published, 'production must serve the previously built bundle');
    assert.equal((await request('/.tachyon/hot')).status, 404, 'production must not expose HMR');
  });
  assert.equal(await readFile(output, 'utf8'), published, 'start must not rebuild sources');
  console.log('PASS: counted loops, single entity decoding, route identity, viewport, immutable production start');
} finally {
  await rm(root, { recursive: true, force: true });
}
