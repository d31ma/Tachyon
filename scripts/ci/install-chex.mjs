// Install only the immutable, checksum-pinned validator used by CI.
import { createHash } from 'node:crypto';
import { chmod, mkdir, writeFile, appendFile, rm } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { execFileSync } from 'node:child_process';

/** @type {Record<string, [string, string]>} */
const targets = {
  'linux-arm64': ['chex-linux-arm64', '1ec31aa7201d6ab3af8114725406050dd48fc6ca052c74a7be378510a1ec96af'],
  'linux-x64': ['chex-linux-x64', 'ed0b71cb5d75a35e13e29b4a160e68d6f3e6221e3da6d80ca5a0361f92e5579b'],
  'darwin-arm64': ['chex-macos-arm64', 'a3869779fdc12210fdf9c8cc54d4ea136912516a76de47621d304dbbeaac47ce'],
  'darwin-x64': ['chex-macos-x64', '1b410bf166ffddf2921f661a18c3e90ba2ab74818f7a125fff4d20f0c3566806'],
  'win32-x64': ['chex-windows-x64.exe', '3aa465447849d1f0d43318cd7c0e3c69a7db8cc06055a0c6ba0b4d53c24334bc'],
};
const target = targets[`${process.platform}-${process.arch}`];
if (!target) throw new Error('No qualified CHEX binary for this runner');
const directoryArgument = process.argv[2];
if (!directoryArgument) throw new Error('Pass an isolated installation directory');
const directory = resolve(directoryArgument);
const [asset, digest] = target;
const response = await fetch(`https://github.com/d31ma/CHEX/releases/download/v26.32.02/${asset}`, {
  signal: AbortSignal.timeout(30_000),
});
if (!response.ok || !response.body) throw new Error(`CHEX download failed (${response.status})`);
const chunks = [];
let length = 0;
for await (const chunk of response.body) {
  length += chunk.length;
  if (length > 4 * 1024 * 1024) throw new Error('CHEX download exceeds its size budget');
  chunks.push(chunk);
}
const bytes = Buffer.concat(chunks);
if (createHash('sha256').update(bytes).digest('hex') !== digest) {
  throw new Error('CHEX checksum mismatch; refusing to write or execute it');
}
await mkdir(directory, { recursive: true });
const executable = join(directory, process.platform === 'win32' ? 'chex.exe' : 'chex');
await writeFile(executable, bytes, { flag: 'wx', mode: 0o755 });
await chmod(executable, 0o755);
const schema = join(directory, 'probe.schema.json');
await writeFile(schema, '{"value":"^ok$"}', { flag: 'wx', mode: 0o600 });
try {
  const answer = JSON.parse(execFileSync(executable, ['validate', schema, '{"value":"ok"}'], {
    encoding: 'utf8', timeout: 10_000, maxBuffer: 64 * 1024,
  }));
  if (answer.protocolVersion !== 1 || answer.op !== 'validate' || answer.ok !== true) {
    throw new Error('Pinned CHEX failed its validation capability probe');
  }
} finally {
  await rm(schema);
}
if (process.env.GITHUB_PATH) await appendFile(process.env.GITHUB_PATH, `${directory}\n`);
console.log(`Verified CHEX v26.32.02 ${asset}: checksum and validation protocol`);
