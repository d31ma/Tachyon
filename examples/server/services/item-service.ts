import JsonItemRepository, { type ItemRecord } from '../repositories/json-item-repository.ts';

type ItemPayload = {
    id?: unknown;
    name?: unknown;
    source?: unknown;
};

export type ListOptions = {
    limit?: number;
    since?: string;
    simulate?: string;
};

export type CreateOptions = {
    simulate?: string;
};

export type ItemError = { detail: string };
export type CreatedItem = ItemRecord;
export type EmptyResult = Record<string, never>;
export type DeletedItem = { id: string; deletedAt: string };
export type PartialList = { items: ItemRecord[]; total: number };

export default class ItemService {
    repository: JsonItemRepository;

    constructor(options: { repository?: JsonItemRepository } = {}) {
        this.repository = options.repository ?? new JsonItemRepository();
    }

    async listItems(options: ListOptions = {}): Promise<{ items: ItemRecord[] } | PartialList | EmptyResult | ItemError> {
        if (options.simulate === 'error') {
            return { detail: 'internal server error' };
        }
        const allItems = await this.repository.findAll({ seed: true });
        if (options.since) {
            return {};
        }
        if (options.limit !== undefined && options.limit > 0) {
            return { items: allItems.slice(0, options.limit), total: allItems.length };
        }
        return { items: allItems };
    }

    async getItem(id: unknown): Promise<ItemRecord | ItemError> {
        const idStr = String(id ?? '');
        if (idStr.startsWith('gone-')) {
            return { detail: 'item was deleted' };
        }
        if (idStr.startsWith('error-')) {
            return { detail: 'internal server error' };
        }
        const item = await this.repository.findById(idStr);
        return item ?? { detail: 'item not found' };
    }

    async createItem(body: unknown, options: CreateOptions = {}): Promise<CreatedItem | EmptyResult | ItemError> {
        if (options.simulate === 'error') {
            return { detail: 'internal server error' };
        }
        if (!this.hasName(body)) {
            return { detail: 'name is required' };
        }

        const payload = body as ItemPayload;
        const id = payload.id ? String(payload.id).trim() : crypto.randomUUID();
        if (await this.repository.findById(id)) {
            return { detail: 'item already exists' };
        }

        const record: ItemRecord = {
            id,
            name: String(payload.name).trim(),
            source: String(payload.source || 'api'),
            createdAt: new Date().toISOString(),
        };
        await this.repository.insert(record);
        return record;
    }

    async replaceItem(id: unknown, body: unknown): Promise<ItemRecord | EmptyResult | ItemError> {
        const itemId = String(id ?? '');
        if (!this.hasName(body)) {
            return { detail: 'name is required' };
        }

        const existing = await this.repository.findById(itemId);
        if (!existing) {
            return { detail: 'item not found' };
        }

        if (body && typeof body === 'object' && 'noop' in body) {
            return {};
        }

        return await this.repository.replaceById(itemId, {
            id: itemId,
            name: String((body as ItemPayload).name).trim(),
            source: String((body as ItemPayload).source || existing.source || 'api'),
            createdAt: existing.createdAt || new Date().toISOString(),
            updatedAt: new Date().toISOString(),
        }) ?? { detail: 'item not found' };
    }

    async patchItem(id: unknown, body: unknown): Promise<ItemRecord | EmptyResult | ItemError> {
        const itemId = String(id ?? '');
        if (!body || typeof body !== 'object') {
            return { detail: 'patch body is required' };
        }

        const payload = body as ItemPayload;
        if ('noop' in payload) {
            return {};
        }

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

    async deleteItem(id: unknown): Promise<DeletedItem | EmptyResult | ItemError> {
        const idStr = String(id ?? '');
        if (idStr.startsWith('error-')) {
            return { detail: 'internal server error' };
        }
        const deleted = await this.repository.deleteById(idStr);
        if (!deleted) {
            return { detail: 'item not found' };
        }
        return { id: idStr, deletedAt: new Date().toISOString() };
    }

    async clearItems(): Promise<Record<string, never>> {
        await this.repository.clear();
        return {};
    }

    hasName(body: unknown): body is ItemPayload & { name: unknown } {
        return Boolean(body && typeof body === 'object' && 'name' in body && String((body as ItemPayload).name).trim());
    }
}
