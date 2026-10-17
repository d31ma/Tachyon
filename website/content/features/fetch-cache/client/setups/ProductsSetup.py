@setup
class ProductsSetup:
    @staticmethod
    def configure(execution: object) -> dict[str, object]:
        return {'http': {'products': ProductsSetup}}

    @staticmethod
    async def exchange(request: object, execution: object) -> dict[str, list[str]]:
        return {'items': ['anvil', 'hammer']}
