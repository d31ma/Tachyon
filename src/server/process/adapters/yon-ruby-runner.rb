#!/usr/bin/env ruby
require "json"

class YonRubyRunner
  def self.route_class_name(handler_path)
    File.basename(handler_path).split(".", 2).first
  end

  def self.resolve_handler(handler_path)
    if Object.private_method_defined?(:handler)
      return proc { |request| Object.new.send(:handler, request) }
    end

    route_class = route_class_name(handler_path)
    if Object.const_defined?(route_class)
      instance = Object.const_get(route_class).new
      return proc { |request| instance.handler(request) } if instance.respond_to?(:handler)
    end

    raise "Ruby route must define handler(request) or a method-named class with handler(request)"
  end

  def self.write(value)
    return if value.nil?
    $stdout.write(value.is_a?(String) ? value : JSON.generate(value))
  end

  def self.run
    handler_path = ARGV.fetch(0)
    input = $stdin.read
    request = JSON.parse(input.empty? ? "{}" : input)
    load handler_path
    write(resolve_handler(handler_path).call(request))
  end
end

begin
  YonRubyRunner.run
rescue StandardError => error
  $stderr.write(error.message)
  exit 1
end
