import ItemService, { type ListOptions, type CreateOptions } from '@/services/item-service.ts'

type YonRequest = {
  body?: unknown
  query?: Record<string, unknown>
}

export class Handler {
  private static readonly service = new ItemService()

  static async GET(request: YonRequest): Promise<unknown> {
    return Handler.service.listItems(Handler.listOptions(request))
  }

  static async POST(request: YonRequest): Promise<Record<string, unknown>> {
    return Handler.service.createItem(request.body, Handler.createOptions(request))
  }

  static async DELETE(): Promise<Record<string, never>> {
    return Handler.service.clearItems()
  }

  private static listOptions(request: YonRequest): ListOptions {
    const query = request.query ?? {}
    const options: ListOptions = {}
    if (query.limit !== undefined) options.limit = Number(query.limit)
    if (query.since !== undefined) options.since = String(query.since)
    if (query.simulate === 'error') options.simulate = 'error'
    return options
  }

  private static createOptions(request: YonRequest): CreateOptions {
    const query = request.query ?? {}
    const options: CreateOptions = {}
    if (query.simulate === 'error') options.simulate = 'error'
    return options
  }
}
