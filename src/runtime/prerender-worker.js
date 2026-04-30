#!/usr/bin/env bun
// @ts-check
import Compiler from '../compiler/index.js';
const payload = await Bun.stdin.json();
const html = await Compiler.renderPageDocumentForWorker(payload.distPath, payload.pathname, payload.shellHTML, payload.layoutMapping);
Bun.stdout.write(html);
