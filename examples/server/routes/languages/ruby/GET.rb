require_relative '../../../services/ruby_language_service'

def handler(request)
  RubyLanguageService.new.describe(request)
end
