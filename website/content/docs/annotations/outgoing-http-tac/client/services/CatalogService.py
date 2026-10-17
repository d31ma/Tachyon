from typing import Annotated


@service
class CatalogService:
    catalog_client: Annotated[CatalogClient, needs]
    count: int = 0

    async def load(self) -> None:
        answer = await self.catalog_client.all()
        self.count = len(answer['items'])
