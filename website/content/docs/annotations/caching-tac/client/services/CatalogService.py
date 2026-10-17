@service
@cache_settings({"name":"catalog","ttlMs":30000,"maxBytes":65536,"maxEntries":256})
class CatalogService:
    sku: str = 'anvil'
    reads: int = 0

    @on_start
    async def open(self) -> None:
        self.behaviors = await Behaviors.create(CatalogSetup, {'partition': 'public-catalog', 'signal': BehaviorSignal()})

    @cached
    async def find(self, sku: str) -> dict[str, object]:
        self.reads += 1
        return self._entry(sku, self.reads)

    @update_cache({"method":"find"})
    async def write(self, sku: str, reads: int) -> dict[str, object]:
        return self._entry(sku, reads)

    @clear_cache({"method":"find"})
    async def invalidate(self, sku: str) -> dict[str, object]:
        return self._entry(sku, self.reads)

    def _entry(self, sku: str, reads: int) -> dict[str, object]:
        return {'sku': sku, 'reads': reads}
