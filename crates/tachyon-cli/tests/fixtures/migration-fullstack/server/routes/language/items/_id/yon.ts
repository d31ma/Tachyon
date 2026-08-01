import ItemService from '@/services/item-service.ts'

type YonRequest = {
  body?: unknown
  paths?: {
    id?: unknown
  }
}

export class Handler {
  private static readonly service = new ItemService()

  static async GET(request: YonRequest): Promise<Record<string, unknown>> {
    return Handler.service.getItem(Handler.itemId(request))
  }

  static async PUT(request: YonRequest): Promise<Record<string, unknown>> {
    return Handler.service.replaceItem(Handler.itemId(request), request.body)
  }

  static async PATCH(request: YonRequest): Promise<Record<string, unknown>> {
    return Handler.service.patchItem(Handler.itemId(request), request.body)
  }

  static async DELETE(request: YonRequest): Promise<Record<string, unknown>> {
    return Handler.service.deleteItem(Handler.itemId(request))
  }

  private static itemId(request: YonRequest): unknown {
    return request.paths?.id
  }
}
