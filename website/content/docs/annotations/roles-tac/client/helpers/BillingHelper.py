@helper
class BillingHelper:
    def settle(self, subtotal: int) -> int:
        return subtotal + 2
