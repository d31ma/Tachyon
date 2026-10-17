from typing import Protocol


@http_client
class CatalogClient(Protocol):
    @get_request("/api/products")
    async def all(self) -> list[str]: ...
