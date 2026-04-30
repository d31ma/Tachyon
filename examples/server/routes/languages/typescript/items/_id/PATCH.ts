import ItemService from '../../../../../services/item-service.ts'

type YonRequest = {
  body?: unknown
  paths?: {
    id?: unknown
  }
}

const service = new ItemService()

export async function handler(request: YonRequest): Promise<Record<string, unknown>> {
  return service.patchItem(request.paths?.id, request.body)
}
