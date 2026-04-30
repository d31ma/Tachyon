export type ItemRecord = {
    id: string;
    name: string;
    source: string;
    createdAt: string;
    updatedAt?: string;
};

export default class JsonItemRepository {
    static defaultSeedItems: ItemRecord[] = [
        { id: 'seed-widget', name: 'Widget', source: 'seed', createdAt: '2026-04-21T09:00:00.000Z' },
        { id: 'seed-gadget', name: 'Gadget', source: 'seed', createdAt: '2026-04-21T09:05:00.000Z' },
    ];

    dataPath: string;
    seedItems: ItemRecord[];

    constructor(options: { dataPath?: string; seedItems?: ItemRecord[] } = {}) {
        this.dataPath = options.dataPath ?? process.env.YON_ITEMS_DATA_PATH ?? `${process.cwd()}/server/data/items.json`;
        this.seedItems = options.seedItems ?? JsonItemRepository.defaultSeedItems;
    }

    async ensureStore(options: { seed?: boolean } = {}): Promise<void> {
        const shouldSeed = options.seed !== false;
        const file = Bun.file(this.dataPath);
        if (!await file.exists()) {
            await Bun.write(this.dataPath, JSON.stringify(shouldSeed ? this.seedItems : [], null, 2));
        }
    }

    async findAll(options: { seed?: boolean } = {}): Promise<ItemRecord[]> {
        await this.ensureStore(options);
        const items: unknown = await Bun.file(this.dataPath).json();
        return Array.isArray(items) ? items.filter(JsonItemRepository.isItemRecord) : [];
    }

    async findById(id: string): Promise<ItemRecord | null> {
        const items = await this.findAll({ seed: true });
        return items.find((item) => item.id === id) ?? null;
    }

    async insert(item: ItemRecord): Promise<void> {
        const items = await this.findAll({ seed: true });
        await this.saveAll([item, ...items]);
    }

    async replaceById(id: string, replacement: ItemRecord): Promise<ItemRecord | null> {
        const items = await this.findAll({ seed: true });
        const index = items.findIndex((item) => item.id === id);
        if (index === -1) return null;

        items[index] = replacement;
        await this.saveAll(items);
        return replacement;
    }

    async updateById(id: string, patch: Partial<Pick<ItemRecord, 'name' | 'source'>>): Promise<ItemRecord | null> {
        const existing = await this.findById(id);
        if (!existing) return null;

        const updated: ItemRecord = {
            ...existing,
            ...patch,
            id,
            updatedAt: new Date().toISOString(),
        };
        return this.replaceById(id, updated);
    }

    async deleteById(id: string): Promise<boolean> {
        const items = await this.findAll({ seed: true });
        const nextItems = items.filter((item) => item.id !== id);
        if (nextItems.length === items.length) return false;

        await this.saveAll(nextItems);
        return true;
    }

    async saveAll(items: ItemRecord[]): Promise<void> {
        await this.ensureStore({ seed: false });
        await Bun.write(this.dataPath, JSON.stringify(items, null, 2));
    }

    async clear(): Promise<void> {
        await this.saveAll([]);
    }

    static isItemRecord(value: unknown): value is ItemRecord {
        return Boolean(
            value
            && typeof value === 'object'
            && typeof (value as ItemRecord).id === 'string'
            && typeof (value as ItemRecord).name === 'string'
            && typeof (value as ItemRecord).source === 'string'
            && typeof (value as ItemRecord).createdAt === 'string',
        );
    }
}
