// @ts-check
import { expect, test } from 'bun:test';
import path from 'path';

class LegacyTacPrefixAudit {
    static sourceGlobs = [
        'src/compiler/**/*.js',
        'src/runtime/**/*.js',
        'src/server/http/browser-env.js',
        'src/types/globals.d.ts',
        'website/client/**/*.{css,html,js,ts}',
    ];

    static legacyPattern = /__ty|\bty_|["'`]ty-/;

    static async findings() {
        /** @type {Array<{ file: string, line: number, text: string }>} */
        const findings = [];
        for (const pattern of this.sourceGlobs) {
            const glob = new Bun.Glob(pattern);
            for await (const relativePath of glob.scan({ cwd: process.cwd(), onlyFiles: true })) {
                if (relativePath.includes('/shared/assets/duvay/'))
                    continue;
                const source = await Bun.file(path.join(process.cwd(), relativePath)).text();
                source.split('\n').forEach((line, index) => {
                    if (this.legacyPattern.test(line))
                        findings.push({ file: relativePath, line: index + 1, text: line.trim() });
                });
            }
        }
        return findings;
    }
}

test('Tac internals use the tc prefix exclusively', async () => {
    expect(await LegacyTacPrefixAudit.findings()).toEqual([]);
});
