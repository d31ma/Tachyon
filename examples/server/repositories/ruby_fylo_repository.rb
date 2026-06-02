require 'json'
require 'open3'

class RubyFyloRepository
  def initialize(root = nil)
    @root = root || ENV.fetch('FYLO_ROOT', File.expand_path('db', Dir.pwd))
    @executable = ENV['FYLO_EXEC_PATH']
  end

  def machine(request)
    command = @executable ? [@executable, 'exec'] : ['bunx', '--bun', 'fylo.exec', 'exec']
    stdout, stderr, status = Open3.capture3(
      *command, '--request', '-', '--root', @root,
      stdin_data: JSON.generate(request)
    )
    raise(stderr.empty? ? stdout : stderr) unless status.success?

    response = JSON.parse(stdout.empty? ? '{}' : stdout)
    raise(response.fetch('error', {}).fetch('message', 'fylo.exec returned an error')) unless response['ok']

    response['result']
  end

  def write_sample(language, request_id)
    collection = 'language-route-events'
    machine({ op: 'createCollection', collection: collection })
    document = {
      language: language,
      source: 'fylo.exec',
      requestId: request_id
    }
    doc_id = machine({ op: 'putData', collection: collection, data: document })
    found = machine({
      op: 'findDocs',
      collection: collection,
      query: { '$ops' => [{ language: { '$eq' => language } }] }
    })
    {
      collection: collection,
      id: doc_id,
      document: document,
      matched: found.is_a?(Hash) ? found.length.to_s : '0',
      operations: ['createCollection', 'putData', 'findDocs'],
      resultCount: '3'
    }
  end

  def query_with_sql(language, request_id)
    collection = 'language-route-events'
    machine({ op: 'createCollection', collection: collection })
    machine({
      op: 'putData',
      collection: collection,
      data: {
        language: language,
        source: 'fylo.exec',
        requestId: request_id,
        query: 'sql'
      }
    })
    rows = machine({ op: 'executeSQL', sql: "SELECT * FROM #{collection}" })
    {
      collection: collection,
      operations: ['createCollection', 'putData', 'executeSQL'],
      resultCount: rows.is_a?(Array) ? rows.length.to_s : '1'
    }
  end
end
