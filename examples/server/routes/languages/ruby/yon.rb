require_relative '../../../services/ruby_language_service'

class Handler
  STATUS_RESPONSES = {
    '401' => { code: '401', detail: 'unauthorized' },
    '402' => { code: '402', detail: 'payment required' },
    '403' => { code: '403', detail: 'forbidden' },
    '404' => { code: '404', detail: 'not found' },
    '405' => { code: '405', detail: 'method not allowed' }
  }.freeze
  private_constant :STATUS_RESPONSES

  def self.GET(request)
    code = status_code(request)
    return STATUS_RESPONSES[code] if STATUS_RESPONSES.key?(code)

    service.describe(request)
  end

  def self.status_code(request)
    query = request['query'] || {}
    query['code'] ? query['code'].to_s : ''
  end

  def self.service
    @service ||= RubyLanguageService.new
  end

  private_class_method :status_code, :service
end
