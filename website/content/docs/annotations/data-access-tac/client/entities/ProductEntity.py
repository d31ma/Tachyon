from typing import Annotated


@entity({"adapter":"publicCatalog"})
@table("products")
class ProductEntity:
    id: Annotated[int, id] = 0
    status: str = ''
