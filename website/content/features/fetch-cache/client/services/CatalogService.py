from typing import Annotated


@service
class CatalogService:
    products_client: Annotated[ProductsClient, needs]
    count: int = 0

    async def load(self) -> None:
        answer = await self.products_client.list()
        self.count = len(answer['items'])

    async def add(self) -> None:
        await self.products_client.save({'name': 'anvil'})
