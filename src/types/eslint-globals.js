// @ts-check
/**
 * ESLint flat-config globals for Tac companion scripts.
 *
 * TypeScript-aware tooling should use `@d31ma/tachyon/globals`; ESLint's
 * `no-undef` rule needs this runtime config because it does not read `.d.ts`
 * ambient declarations.
 */
export default {
    Tac: 'readonly',
    env: 'readonly',
    onMount: 'readonly',
    publish: 'readonly',
    subscribe: 'readonly',
    fylo: 'readonly',
    json: 'readonly',
    Worker: 'readonly',
};
