import TypeScriptLanguageService, { type YonRequest } from '@/services/typescript-language-service.ts'

const service = new TypeScriptLanguageService()

const statusResponses: Record<string, Record<string, unknown>> = {
  '207': { code: '207', message: 'multi-status' },
  '208': { code: '208', message: 'already reported' },
  '226': { code: '226', message: 'im used' },
  '300': { code: '300', detail: 'multiple choices' },
  '301': { code: '301', location: '/redirect' },
  '302': { code: '302', location: '/redirect' },
}

export async function handler(request: YonRequest) {
  const raw = request.query?.code
  const code = typeof raw === 'number' ? String(Math.floor(raw)) : typeof raw === 'string' ? raw : ''
  if (code && statusResponses[code])
    return statusResponses[code]
  return service.describe(request)
}
