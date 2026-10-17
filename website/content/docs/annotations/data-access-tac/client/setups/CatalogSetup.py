@setup
class CatalogSetup:
    @staticmethod
    def configure(execution: object) -> dict[str, object]:
        return {'entities': {'publicCatalog': CatalogSetup}, 'authorizeEntity': CatalogSetup.authorize}

    @staticmethod
    async def execute(command: dict[str, object], execution: object) -> object:
        if command['query']['kind'] == 'count':
            return 0
        return []

    @staticmethod
    async def authorize(command: dict[str, object], execution: object) -> bool:
        return command['entity'] == 'ProductEntity'
