import ItemService from '../../../../services/item-service.ts'

const service = new ItemService()

export async function handler(): Promise<Record<string, never>> {
  return service.clearItems()
}
