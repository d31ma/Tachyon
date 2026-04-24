#!/usr/bin/env bun
// @ts-check
import Tac from '../compiler/template-compiler.js';
const payload = await Bun.stdin.json();
const html = await Tac.renderPageDocumentForWorker(payload.distPath, payload.pathname, payload.shellHTML, payload.layoutMapping);
Bun.stdout.write(html);
