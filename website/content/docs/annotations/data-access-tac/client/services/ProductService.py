from typing import Annotated


@service
class ProductService:
    product_store: Annotated[ProductStore, needs]
    summary: str = 'Not loaded.'

    async def load(self) -> None:
        rows = await self.product_store.excluding('retired', {'size': 5})
        active_count = await self.product_store.count_by_status('active')
        self.summary = f'{len(rows)} of {active_count} active'
