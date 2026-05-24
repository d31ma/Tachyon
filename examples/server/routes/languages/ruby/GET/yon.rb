require_relative '../../../../services/ruby_language_service'

STATUS_RESPONSES = {
  '401' => { code: '401', detail: 'unauthorized' },
  '402' => { code: '402', detail: 'payment required' },
  '403' => { code: '403', detail: 'forbidden' },
  '404' => { code: '404', detail: 'not found' },
  '405' => { code: '405', detail: 'method not allowed' }
}

def handler(request)
  query = request['query'] || {}
  code = query['code'] ? query['code'].to_s : ''
  return STATUS_RESPONSES[code] if STATUS_RESPONSES.key?(code)

  RubyLanguageService.new.describe(request)
end
