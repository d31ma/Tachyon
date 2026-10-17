from typing import Annotated


@service
class BillingService:
    billing_helper: Annotated[BillingHelper, needs]
    total: int = 0
    _SUBTOTAL = 40

    @on_start
    def open(self) -> None:
        self.total = self.billing_helper.settle(self._SUBTOTAL)
