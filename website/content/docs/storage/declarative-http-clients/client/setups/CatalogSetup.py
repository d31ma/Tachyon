@setup
class CatalogSetup:
    @staticmethod
    def configure(execution: object) -> dict[str, object]:
        return {'http': {'catalog': CatalogSetup}}

    @staticmethod
    async def exchange(request: object, execution: object) -> list[str]:
        return ['anvil', 'rope']
