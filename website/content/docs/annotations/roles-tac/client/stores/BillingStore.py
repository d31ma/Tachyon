from typing import Protocol


@store(BillingEntity, {"fields":["id"],"defaultSize":20,"maxSize":100})
class BillingStore(Repository[BillingEntity, int], Protocol):
    async def exists_by_id(self, id: int) -> bool: ...
