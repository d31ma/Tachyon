import TypeScriptLanguageService, { type YonRequest } from '../../../services/typescript-language-service.ts'

const service = new TypeScriptLanguageService()

export async function handler(request: YonRequest) {
  return service.describe(request)
}
