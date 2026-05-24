import ItemService, { type CreateOptions } from '@/services/item-service.ts'

type YonRequest = {
  body?: unknown
  query?: Record<string, unknown>
}

const service = new ItemService()

export async function handler(request: YonRequest): Promise<Record<string, unknown>> {
  const query = request.query ?? {}
  const options: CreateOptions = {}
  if (query.simulate === 'error') options.simulate = 'error'
  return service.createItem(request.body, options)
}
