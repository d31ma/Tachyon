import TypeScriptLanguageService, { type YonRequest } from '@/services/typescript-language-service.ts'

export class Handler {
  private static readonly service = new TypeScriptLanguageService()

  private static readonly statusResponses: Record<string, Record<string, unknown>> = {
    '207': { code: '207', message: 'multi-status' },
    '208': { code: '208', message: 'already reported' },
    '226': { code: '226', message: 'im used' },
    '300': { code: '300', detail: 'multiple choices' },
    '301': { code: '301', location: '/redirect' },
    '302': { code: '302', location: '/redirect' },
  }

  static async GET(request: YonRequest) {
    const code = Handler.statusCode(request)
    if (code && Handler.statusResponses[code])
      return Handler.statusResponses[code]
    return Handler.service.describe(request)
  }

  private static statusCode(request: YonRequest): string {
    const raw = request.query?.code
    if (typeof raw === 'number') return String(Math.floor(raw))
    return typeof raw === 'string' ? raw : ''
  }
}
