// @ts-nocheck
// CHEX client — drives the `chex` binary's persistent NDJSON loop.
//
// For JS/TS apps that consume the compiled binary instead of importing a
// package. No dependencies (node:child_process only). Requires the `chex`
// binary on PATH or an explicit path. One long-lived subprocess.
//
//   import { CHEX } from './chex.mjs'
//   const c = new CHEX()
//   const data = await c.validate('./schemas/person.schema.json', { name: 'Ada' })
//   const data2 = await c.validate('person', { name: 'Ada' }, './schemas')
//   await c.close()
//
// `validate` resolves with the validated data and rejects when it does not match
// the schema. `request(op)` is a raw escape hatch resolving the full response.
// Requests are queued: each resolves with its own response line, in order.

import NdjsonProcessClient from '../shared/ndjson-process-client.mjs'

export class CHEX extends NdjsonProcessClient {
    /** @param {{ binary?: string }} [opts] */
    constructor(opts = {}) {
        super({
            name: 'chex',
            command: NdjsonProcessClient.resolveCommand('chex', opts.binary),
            args: ['exec', '--loop'],
        })
    }

    async _op(op, fields) {
        const payload = { op }
        for (const [key, value] of Object.entries(fields)) {
            if (value !== undefined) payload[key] = value
        }
        const response = await this.request(payload)
        if (!response.ok) throw new Error(response.error?.message ?? 'chex error')
        return response.result
    }

    /** Validate data against a schema (name or .schema.json path). Returns the validated data. */
    validate(schema, data, schemaDir) {
        return this._op('validate', { schema, data, schemaDir })
    }

}
