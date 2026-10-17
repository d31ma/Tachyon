from typing import Annotated


@service
class CounterService:
    count: int = 0
    visits: Annotated[int, session_state] = 0
    theme: Annotated[str, local_state] = "light"

    def increment(self) -> None:
        self.count += 1
