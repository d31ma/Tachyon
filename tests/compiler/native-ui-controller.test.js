// @ts-check
import { afterAll, expect, test } from 'bun:test';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import NativeUIControllerCompiler from '../../src/compiler/native-ui/controller-compiler.js';

/** @type {string[]} */
const tempDirs = [];
afterAll(async () => {
    await Promise.all(tempDirs.splice(0).map((directory) => rm(directory, { recursive: true, force: true })));
});

test('bundles route render closures into one DOM-free native controller program', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-native-controller-'));
    tempDirs.push(root);
    const pageModule = path.join(root, 'page.js');
    const outputFile = path.join(root, 'tachyon.native-controller.js');
    await writeFile(pageModule, `
export default async function () {
  let count = 0;
  return async (elementId, event) => {
    if (elementId === 'increment' && event?.type === 'click') count += 1;
    return '<main><p>Count: ' + count + '</p><button id="increment" data-tac-on-click="">Add</button></main>';
  };
}`);

    await NativeUIControllerCompiler.compile({
        routes: [{ route: '/', modulePath: pageModule }],
        outputFile,
        adapters: [],
    });
    const source = await readFile(outputFile, 'utf8');
    expect(source).not.toMatch(/^\s*(?:import|export)\s/m);
    expect(source).not.toContain('import.meta');
    expect(source).not.toMatch(/\b\d+n\b/);

    const runner = path.join(root, 'runner.js');
    await writeFile(runner, `${source}
const initial = await globalThis.__tachyonNativeUI.render();
const updated = await globalThis.__tachyonNativeUI.dispatch(JSON.stringify({ elementId: 'increment', type: 'click' }));
console.log(JSON.stringify({ initial: JSON.parse(initial), updated: JSON.parse(updated) }));`);
    const processHandle = Bun.spawn(['bun', runner], { stdout: 'pipe', stderr: 'pipe' });
    const [stdout, stderr, exitCode] = await Promise.all([
        new Response(processHandle.stdout).text(),
        new Response(processHandle.stderr).text(),
        processHandle.exited,
    ]);
    if (exitCode !== 0) throw new Error(stderr);
    const result = JSON.parse(stdout);
    expect(JSON.stringify(result.initial)).toContain('Count: 0');
    expect(JSON.stringify(result.updated)).toContain('Count: 1');

    const javaScriptCore = Bun.which('jsc');
    if (javaScriptCore) {
        const jscRunner = path.join(root, 'jsc-runner.js');
        await writeFile(jscRunner, `${source}
__tachyonNativeUI.render().then((initial) => {
  print(initial);
  return __tachyonNativeUI.dispatch(JSON.stringify({ elementId: 'increment', type: 'click' }));
}).then(print);`);
        const jsc = Bun.spawn([javaScriptCore, jscRunner], { stdout: 'pipe', stderr: 'pipe' });
        const [jscStdout, jscStderr, jscExitCode] = await Promise.all([
            new Response(jsc.stdout).text(), new Response(jsc.stderr).text(), jsc.exited,
        ]);
        if (jscExitCode !== 0) throw new Error(jscStderr);
        expect(jscStdout).toContain('Count: 0');
        expect(jscStdout).toContain('Count: 1');
    }
});

test('resolves compiler-rooted component imports from the target bundle', async () => {
    const root = await mkdtemp(path.join(tmpdir(), 'tachyon-native-controller-rooted-'));
    tempDirs.push(root);
    const component = path.join(root, 'components', 'chart', 'tac.js');
    const pageModule = path.join(root, 'pages', 'tac.js');
    const outputFile = path.join(root, 'tachyon.native-controller.js');
    await mkdir(path.dirname(component), { recursive: true });
    await mkdir(path.dirname(pageModule), { recursive: true });
    await writeFile(component, 'export default async () => async () => "<company-chart></company-chart>";');
    await writeFile(pageModule, `
const loadChart = () => import('/components/chart/tac.js');
export default async function () {
  await loadChart();
  return async () => '<main><p>Dashboard</p></main>';
}`);

    await NativeUIControllerCompiler.compile({
        routes: [{ route: '/', modulePath: pageModule }], outputFile,
    });
    expect(await Bun.file(outputFile).exists()).toBe(true);
    expect(await readFile(outputFile, 'utf8')).toContain('company-chart');
});
