#!/usr/bin/env bun
// @ts-check
//
// `ty` — the single Tachyon CLI, compiled to a standalone per-OS/arch binary
// (`bun build --compile`). Tachyon is binary-first now: no npm package, no
// `bun` on the user's machine. This dispatcher routes `ty <command> …` to the
// command modules, which read their flags from `process.argv`.
//
//   ty init [name] [--target <t>]     scaffold a new app
//   ty serve [--port <n>] …           run the dev/prod server
//   ty bundle [--watch] …             build client + native artifacts
//   ty native-bundle [--target <t>]   generate the native host only
//   ty preview [--watch] …            preview a built bundle
//
// The command modules run on import (top-level await + side effects), so we
// normalize argv to `[exec, <command>, …args]` first — giving them the same
// shape they'd see under `bun src/cli/<command>.js …` (flags from index 2).

import pkg from '../../package.json';

const COMMANDS = ['init', 'serve', 'bundle', 'native-bundle', 'preview'];

function usage() {
    console.log(`ty ${pkg.version} — Tachyon CLI

Usage: ty <command> [options]

Commands:
  init [name]        Scaffold a new Tachyon app
  serve              Run the server (dev or production)
  bundle             Build client + native artifacts
  native-bundle      Generate the native host only
  preview            Preview a built bundle

Run 'ty <command> --help' for command-specific options.`);
}

// Bun puts the entry at argv[1] in both modes — the real script under
// `bun src/cli/index.js …`, and the virtual `/$bunfs/root/ty` in a compiled
// binary — so the user's args are always argv.slice(2).
const userArgs = process.argv.slice(2);
const command = userArgs[0];
const rest = userArgs.slice(1);

if (!command || command === '--help' || command === '-h' || command === 'help') {
    usage();
    process.exit(command ? 0 : 1);
}
if (command === '--version' || command === '-v') {
    console.log(pkg.version);
    process.exit(0);
}
if (!COMMANDS.includes(command)) {
    console.error(`Unknown command: ${command}\n`);
    usage();
    process.exit(1);
}

// Give the command module the argv shape it expects: [exec, <command>, …args].
process.argv = [process.argv[0], command, ...rest];

// Literal specifiers so `bun build --compile` statically bundles every command.
switch (command) {
    case 'init': await import('./init.js'); break;
    case 'serve': await import('./serve.js'); break;
    case 'bundle': await import('./bundle.js'); break;
    case 'native-bundle': await import('./native-bundle.js'); break;
    case 'preview': await import('./preview.js'); break;
}
