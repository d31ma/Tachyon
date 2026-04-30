export type YonRequest = {
  context?: {
    requestId?: string
  }
}

export default class TypeScriptLanguageService {
  describe(request: YonRequest) {
    return {
      language: 'typescript',
      message: 'Hello from TypeScript!',
      requestId: request.context?.requestId ?? 'unknown',
    }
  }
}
