from typing import Annotated


@service
class CatalogService:
    catalog_client: Annotated[CatalogClient, needs]
    products: list[str] = []
    error: str = ''

    async def load(self) -> None:
        self._show(await self._fetch())

    async def _fetch(self) -> list[str] | None:
        try:
            return await self.catalog_client.all()
        except Exception:
            return None

    def _show(self, answer: list[str] | None) -> None:
        if answer is None:
            self.error = 'Products could not be loaded. Please retry.'
            return
        self.products = answer
        self.error = ''
