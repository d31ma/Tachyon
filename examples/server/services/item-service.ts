import JsonItemRepository, { type ItemRecord } from '../repositories/json-item-repository.ts';

type ItemPayload = {
    id?: unknown;
    name?: unknown;
    source?: unknown;
};

export default class ItemService {
    repository: JsonItemRepository;

    constructor(options: { repository?: JsonItemRepository } = {}) {
        this.repository = options.repository ?? new JsonItemRepository();
    }

    async listItems(): Promise<{ items: ItemRecord[] }> {
        return { items: await this.repository.findAll({ seed: true }) };
    }

    async getItem(id: unknown): Promise<ItemRecord | { detail: string }> {
        const item = await this.repository.findById(String(id ?? ''));
        return item ?? { detail: 'item not found' };
    }

    async createItem(body: unknown): Promise<Record<string, never> | { detail: string }> {
        if (!this.hasName(body)) {
            return { detail: 'name is required' };
        }

        const payload = body;
        const id = payload.id ? String(payload.id).trim() : crypto.randomUUID();
        if (await this.repository.findById(id)) {
            return { detail: 'item already exists' };
        }

        await this.repository.insert({
            id,
            name: String(payload.name).trim(),
            source: String(payload.source || 'api'),
            createdAt: new Date().toISOString(),
        });
        return {};
    }

    async replaceItem(id: unknown, body: unknown): Promise<ItemRecord | { detail: string }> {
        const itemId = String(id ?? '');
        if (!this.hasName(body)) {
            return { detail: 'name is required' };
        }

        const existing = await this.repository.findById(itemId);
        if (!existing) {
            return { detail: 'item not found' };
        }

        return await this.repository.replaceById(itemId, {
            id: itemId,
            name: String(body.name).trim(),
            source: String(body.source || existing.source || 'api'),
            createdAt: existing.createdAt || new Date().toISOString(),
            updatedAt: new Date().toISOString(),
        }) ?? { detail: 'item not found' };
    }

    async patchItem(id: unknown, body: unknown): Promise<ItemRecord | { detail: string }> {
        const itemId = String(id ?? '');
        if (!body || typeof body !== 'object') {
            return { detail: 'patch body is required' };
        }

        const payload = body as ItemPayload;
        const patch: Partial<Pick<ItemRecord, 'name' | 'source'>> = {};
        if (payload.name !== undefined) {
            const name = String(payload.name).trim();
            if (!name) return { detail: 'name is required' };
            patch.name = name;
        }
        if (payload.source !== undefined) {
            patch.source = String(payload.source).trim() || 'api';
        }
        if (Object.keys(patch).length === 0) {
            return { detail: 'patch body is required' };
        }

        return await this.repository.updateById(itemId, patch) ?? { detail: 'item not found' };
    }

    async deleteItem(id: unknown): Promise<Record<string, never> | { detail: string }> {
        return await this.repository.deleteById(String(id ?? ''))
            ? {}
            : { detail: 'item not found' };
    }

    async clearItems(): Promise<Record<string, never>> {
        await this.repository.clear();
        return {};
    }

    hasName(body: unknown): body is ItemPayload & { name: unknown } {
        return Boolean(body && typeof body === 'object' && 'name' in body && String((body as ItemPayload).name).trim());
    }
}
