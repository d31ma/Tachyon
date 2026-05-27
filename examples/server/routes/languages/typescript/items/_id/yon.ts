import ItemService from '@/services/item-service.ts'

type YonRequest = {
  body?: unknown
  paths?: {
    id?: unknown
  }
}

const service = new ItemService()

export class Handler {
  static async GET(request: YonRequest): Promise<Record<string, unknown>> {
    return service.getItem(request.paths?.id)
  }

  static async PUT(request: YonRequest): Promise<Record<string, unknown>> {
    return service.replaceItem(request.paths?.id, request.body)
  }

  static async PATCH(request: YonRequest): Promise<Record<string, unknown>> {
    return service.patchItem(request.paths?.id, request.body)
  }

  static async DELETE(request: YonRequest): Promise<Record<string, unknown>> {
    return service.deleteItem(request.paths?.id)
  }
}
