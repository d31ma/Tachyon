class RubyLanguageService
  def describe(request)
    context = request.fetch('context', {})
    {
      language: 'ruby',
      message: 'Hello from Ruby!',
      requestId: context.fetch('requestId', 'unknown')
    }
  end
end
