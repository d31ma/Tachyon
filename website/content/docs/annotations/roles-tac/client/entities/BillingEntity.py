from typing import Annotated


@entity({"adapter":"publicBilling"})
@table("invoices")
class BillingEntity:
    id: Annotated[int, id] = 0
