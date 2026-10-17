from typing import Annotated, Protocol


@cache_settings({"name":"products","ttlMs":60000,"maxBytes":65536,"maxEntries":16})
@http_client
class ProductsClient(Protocol):
    @cached
    @get_request("/api/products")
    async def list(self) -> dict[str, object]: ...

    @clear_cache({"method":"list"})
    @post_request("/api/products")
    async def save(self, product: Annotated[dict[str, str], request_body]) -> dict[str, object]: ...
