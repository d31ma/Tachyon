// @ts-check
import { afterEach, expect, test } from 'bun:test';
import {
    clearBackends,
    hasInHouseBackend,
    resolveBackend,
    setBackend,
} from '../../../src/server/process/backends/registry.js';

afterEach(() => clearBackends());

test('defaults to the subprocess backend for unregistered handlers', () => {
    expect(resolveBackend('/routes/api/yon.js')).toBe('subprocess');
    expect(hasInHouseBackend('/routes/api/yon.js')).toBe(false);
});

test('records and resolves an explicit backend per handler path', () => {
    setBackend('/routes/rust/yon.rs', 'wasm-compiled');
    setBackend('/routes/py/yon.py', 'wasm-interpreter');

    expect(resolveBackend('/routes/rust/yon.rs')).toBe('wasm-compiled');
    expect(resolveBackend('/routes/py/yon.py')).toBe('wasm-interpreter');
    expect(hasInHouseBackend('/routes/rust/yon.rs')).toBe(true);
    expect(hasInHouseBackend('/routes/py/yon.py')).toBe(true);

    // An unrelated handler is unaffected.
    expect(resolveBackend('/routes/java/yon.java')).toBe('subprocess');
});

test('clearBackends() drops every registration (HMR reset)', () => {
    setBackend('/routes/rust/yon.rs', 'wasm-compiled');
    clearBackends();
    expect(resolveBackend('/routes/rust/yon.rs')).toBe('subprocess');
    expect(hasInHouseBackend('/routes/rust/yon.rs')).toBe(false);
});
