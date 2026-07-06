// @ts-check
import { describe, expect, test } from 'bun:test';
import { ServiceWorkerPolicy } from '../../src/runtime/service-worker-policy.js';

describe('ServiceWorkerPolicy', () => {
    test.each([
        'localhost',
        'dev.localhost',
        '127.0.0.1',
        '127.42.8.9',
        '::1',
        '[::1]',
    ])('treats %s as a loopback host', (hostname) => {
        expect(ServiceWorkerPolicy.isLoopback(hostname)).toBe(true);
        expect(ServiceWorkerPolicy.shouldRegister({
            protocol: 'http:',
            hostname,
        })).toBe(false);
    });

    test('registers only for non-loopback HTTP origins', () => {
        expect(ServiceWorkerPolicy.shouldRegister({
            protocol: 'https:',
            hostname: 'tachyon.del.ma',
        })).toBe(true);
        expect(ServiceWorkerPolicy.shouldRegister({
            protocol: 'file:',
            hostname: '',
        })).toBe(false);
    });

    test('uses the network for navigations and stable runtime entrypoints', () => {
        expect(ServiceWorkerPolicy.strategyFor({
            method: 'GET',
            mode: 'navigate',
            destination: 'document',
            url: 'https://tachyon.del.ma/',
        }, 'https://tachyon.del.ma')).toBe('network-first');
        expect(ServiceWorkerPolicy.strategyFor({
            method: 'GET',
            mode: 'same-origin',
            destination: 'script',
            url: 'https://tachyon.del.ma/imports.js',
        }, 'https://tachyon.del.ma')).toBe('network-first');
    });

    test('caches versioned static assets without intercepting application APIs', () => {
        expect(ServiceWorkerPolicy.strategyFor({
            method: 'GET',
            mode: 'cors',
            destination: 'style',
            url: 'https://tachyon.del.ma/shared/assets/landing.css?v=2',
        }, 'https://tachyon.del.ma')).toBe('cache-first');
        expect(ServiceWorkerPolicy.strategyFor({
            method: 'GET',
            mode: 'cors',
            destination: '',
            url: 'https://tachyon.del.ma/api/items',
        }, 'https://tachyon.del.ma')).toBe('bypass');
        expect(ServiceWorkerPolicy.strategyFor({
            method: 'GET',
            mode: 'cors',
            destination: '',
            url: 'https://tachyon.del.ma/api/items.json',
        }, 'https://tachyon.del.ma')).toBe('bypass');
        expect(ServiceWorkerPolicy.strategyFor({
            method: 'POST',
            mode: 'cors',
            destination: '',
            url: 'https://tachyon.del.ma/api/items',
        }, 'https://tachyon.del.ma')).toBe('bypass');
    });
});
