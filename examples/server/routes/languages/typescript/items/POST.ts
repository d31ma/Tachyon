import ItemService from '../../../../services/item-service.ts'

type YonRequest = {
  body?: unknown
}

const service = new ItemService()

export async function handler(request: YonRequest): Promise<Record<string, unknown>> {
  return service.createItem(request.body)
}
