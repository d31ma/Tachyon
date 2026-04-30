import ItemService from '../../../../../services/item-service.ts'

type YonRequest = {
  paths?: {
    id?: unknown
  }
}

const service = new ItemService()

export async function handler(request: YonRequest): Promise<Record<string, unknown>> {
  return service.deleteItem(request.paths?.id)
}
