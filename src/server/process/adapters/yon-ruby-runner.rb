#!/usr/bin/env ruby
require "json"

class YonRubyRunner
  def self.resolve_handler_class
    unless Object.const_defined?(:Handler)
      raise "Ruby route must define a class named Handler"
    end
    Object.const_get(:Handler)
  end

  def self.resolve_method(handler_class, method)
    unless handler_class.respond_to?(method)
      raise "Handler class does not implement self.#{method}()"
    end
    handler_class.method(method)
  end

  def self.write(value)
    return if value.nil?
    $stdout.write(value.is_a?(String) ? value : JSON.generate(value))
  end

  def self.run
    handler_path = ARGV.fetch(0)
    input = $stdin.read
    request = JSON.parse(input.empty? ? "{}" : input)
    method = request["method"]
    raise "Missing HTTP method in request payload" if method.nil? || method.empty?
    load handler_path
    handler_class = resolve_handler_class
    dispatch = resolve_method(handler_class, method)
    write(dispatch.call(request))
  end
end

begin
  YonRubyRunner.run
rescue StandardError => error
  $stderr.write(error.message)
  exit 1
end
