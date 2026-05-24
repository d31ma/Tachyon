import ItemService, { type ListOptions } from '@/services/item-service.ts'

type YonRequest = {
  query?: Record<string, unknown>
}

const service = new ItemService()

export async function handler(request: YonRequest): Promise<unknown> {
  const query = request.query ?? {}
  const options: ListOptions = {}
  if (query.limit !== undefined) options.limit = Number(query.limit)
  if (query.since !== undefined) options.since = String(query.since)
  if (query.simulate === 'error') options.simulate = 'error'
  return service.listItems(options)
}
