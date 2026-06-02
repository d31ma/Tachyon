require_relative '../repositories/ruby_fylo_repository'

class RubyLanguageService
  def describe(request)
    context = request.fetch('context', {})
    request_id = context.fetch('requestId', 'unknown')
    {
      language: 'ruby',
      message: 'Hello from Ruby!',
      requestId: request_id,
      fylo: repository.query_with_sql('ruby', request_id)
    }
  end

  private

  def repository
    @repository ||= RubyFyloRepository.new
  end
end
