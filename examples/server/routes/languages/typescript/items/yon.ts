import ItemService, { type ListOptions, type CreateOptions } from '@/services/item-service.ts'

type YonRequest = {
  body?: unknown
  query?: Record<string, unknown>
}

const service = new ItemService()

export class Handler {
  static async GET(request: YonRequest): Promise<unknown> {
    const query = request.query ?? {}
    const options: ListOptions = {}
    if (query.limit !== undefined) options.limit = Number(query.limit)
    if (query.since !== undefined) options.since = String(query.since)
    if (query.simulate === 'error') options.simulate = 'error'
    return service.listItems(options)
  }

  static async POST(request: YonRequest): Promise<Record<string, unknown>> {
    const query = request.query ?? {}
    const options: CreateOptions = {}
    if (query.simulate === 'error') options.simulate = 'error'
    return service.createItem(request.body, options)
  }

  static async DELETE(): Promise<Record<string, never>> {
    return service.clearItems()
  }
}
