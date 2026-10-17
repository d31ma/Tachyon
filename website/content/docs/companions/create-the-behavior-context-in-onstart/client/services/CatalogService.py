from typing import Annotated


@service
class CatalogService:
    offset: int = 0

    def page(self, input: Annotated[dict[str, int], page_defaults({"defaultSize":20,"maxSize":100,"sort":["id"]})]) -> int:
        return input['offset']

    @on_start
    async def start(self) -> None:
        self.behaviors = await Behaviors.create(ApplicationSetup, {'partition': 'public-catalog', 'signal': BehaviorSignal()})
        self.offset = self.page({'page': 1})
