@service
class OrdersService:
    attempts: int = 0
    _FIRST_WORKING_ATTEMPT = 3

    @on_start
    async def open(self) -> None:
        self.behaviors = await Behaviors.create(OrdersSetup, {'partition': 'orders', 'signal': BehaviorSignal()})

    @retry({"maxRetries":3,"delayMs":10,"multiplier":2,"maxDelayMs":200,"jitterMs":5})
    async def submit(self, id: str) -> dict[str, object]:
        self.attempts += 1
        self._fail_first_two_attempts()
        return self._report_of(id)

    @max_at_once({"limit":2,"policy":"reject"})
    async def report(self, id: str) -> dict[str, object]:
        return self._report_of(id)

    def _report_of(self, id: str) -> dict[str, object]:
        return {'id': id, 'attempts': self.attempts}

    def _fail_first_two_attempts(self) -> None:
        if self.attempts < self._FIRST_WORKING_ATTEMPT:
            raise RuntimeError('the checkout API is unavailable')
