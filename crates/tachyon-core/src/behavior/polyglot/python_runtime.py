import asyncio as _behavior_asyncio
import builtins as _behavior_builtins
import contextvars as _behavior_contextvars
import inspect as _behavior_inspect
import json as _behavior_json
import math as _behavior_math
import random as _behavior_random
import re as _behavior_re
import sys as _behavior_sys
import time as _behavior_time
import typing as _behavior_typing

class _BehaviorOpenTransactions:
    OPEN = vars(_behavior_sys).setdefault('_tachyon_behavior_transactions', _behavior_contextvars.ContextVar('tachyon_behavior_transactions', default=None))
    @classmethod
    def of(cls, adapter):
        return (cls.OPEN.get() or {}).get(adapter)
    @classmethod
    async def within(cls, adapter, identifier, run):
        joined = dict(cls.OPEN.get() or {})
        joined[adapter] = identifier
        token = cls.OPEN.set(joined)
        try:
            return await run()
        finally:
            cls.OPEN.reset(token)
class _BehaviorLimits:
    MAX_SAFE_INTEGER = 9007199254740991
    MAX_JSON_BYTES = 1048576
    MAX_JSON_DEPTH = 32
    MAX_JSON_NODES = 10000
    MAX_COLLECTION_ITEMS = 10000
    SMALL_JSON_BYTES = 4096
    CONFIGURATION_JSON_BYTES = 65536
    MAX_TEXT_BYTES = 4096
    MAX_PROPERTIES = 256
    MAX_PROPERTY_DEPTH = 16
    MAX_PROPERTY_PATH = 256
    MAX_PAGE = 1000000
    MAX_PAGE_ROWS = 1000000
    MAX_SORT_FIELDS = 16
    MAX_PROFILES = 32
    MAX_SIGNAL_LISTENERS = 128
    MAX_ACTIVE_CALLS = 64
    MAX_ARGUMENTS = 32
    MAX_ADAPTERS = 128
    MAX_PARTITION_BYTES = 256
    MAX_TRANSACTION_ID = 128
    MAX_IN_VALUES = 100
    MAX_PATH_LENGTH = 2048
    MAX_HEADER_NAME = 128
    MAX_ENTITY_ID_BYTES = 256
    ENTITY_ID_JSON_BYTES = 1024
    DEFAULT_TIMEOUT_MS = 10000
    MAX_TIMEOUT_MS = 60000
    DEFAULT_PAGE_SIZE = 20
    MAX_REPOSITORY_PAGE_SIZE = 100

class _BehaviorText:
    UNSAFE_KEYS = ('__proto__', 'prototype', 'constructor', 'behaviors')
    UNSAFE_ADAPTER_NAMES = ('__proto__', 'prototype', 'constructor', 'behaviors', 'tac')
    ADAPTER_GROUPS = ('caches', 'http', 'entities')
    CONTEXT_OPTIONS = ('partition', 'signal', 'timeoutMs', 'profiles', 'properties')
    ADAPTER_OPTIONS = ('caches', 'http', 'entities', 'authorize', 'authorizeEntity')
    IDENTIFIER = r'[A-Za-z_$][A-Za-z0-9_$]{0,127}'
    PROFILE_NAME = r'[A-Za-z_$][A-Za-z0-9_$]{0,63}'
    PROPERTY_KEY = r'[A-Z][A-Z0-9_]{0,255}'
    APPLICATION_KEY = r'[a-z][a-z0-9]*'
    INTEGER = r'-?(0|[1-9][0-9]*)'
    NUMBER = r'-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?'
    WHITESPACE = '\u0009\u000a\u000b\u000c\u000d\u0020\u0085\u00a0\u1680\u2000\u2001\u2002\u2003\u2004\u2005\u2006\u2007\u2008\u2009\u200a\u2028\u2029\u202f\u205f\u3000'
    HEADER_TOKEN = "!#$%&'*+-.^_`|~0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"
    TRANSPORT_HEADERS = ('host', 'connection', 'content-length', 'transfer-encoding', 'cookie')
    READ_METHODS = ('GET', 'HEAD')

class BehaviorError(Exception):
    def __init__(self, code, message, path=None):
        super().__init__(message)
        self.code = code
        self.path = path

def _behavior_invalid(message):
    raise BehaviorError('invalid', message)

class _BehaviorCopier:
    def __init__(self, limit):
        self.limit = limit
        self.nodes = 0
        self.bytes = 0
    def copy(self, item, depth):
        self.nodes += 1
        if depth > _BehaviorLimits.MAX_JSON_DEPTH or self.nodes > _BehaviorLimits.MAX_JSON_NODES:
            _behavior_invalid('Behavior JSON exceeds structural bounds.')
        if item is None or type(item) is bool:
            return item
        if type(item) is int:
            return self.copy_integer(item)
        if type(item) is float and _behavior_math.isfinite(item):
            return item
        if type(item) is str:
            return self.copy_string(item)
        if type(item) is list:
            return self.copy_list(item, depth)
        if type(item) is dict:
            return self.copy_dict(item, depth)
        _behavior_invalid('Behavior values must be finite JSON data.')
    def copy_integer(self, item):
        if abs(item) > _BehaviorLimits.MAX_SAFE_INTEGER:
            _behavior_invalid('Behavior JSON integer exceeds safe bounds.')
        return item
    def copy_string(self, item):
        if len(item) > self.limit - self.bytes:
            _behavior_invalid('Behavior JSON exceeds byte bounds.')
        try:
            self.bytes += len(item.encode('utf-8'))
        except UnicodeEncodeError:
            _behavior_invalid('Behavior strings require valid UTF-8.')
        if self.bytes > self.limit:
            _behavior_invalid('Behavior JSON exceeds byte bounds.')
        return item
    def copy_list(self, item, depth):
        if len(item) > _BehaviorLimits.MAX_COLLECTION_ITEMS:
            _behavior_invalid('Behavior array exceeds bounds.')
        result = []
        for child in item:
            result.append(self.copy(child, depth + 1))
        return result
    def copy_dict(self, item, depth):
        if len(item) > _BehaviorLimits.MAX_COLLECTION_ITEMS or any(type(key) is not str for key in item):
            _behavior_invalid('Behavior JSON requires string object keys.')
        result = {}
        for key in sorted(item):
            if key in _BehaviorText.UNSAFE_KEYS or (key == 'tac' and depth != 0):
                _behavior_invalid('Unsafe behavior JSON field.')
            value = self.copy(item[key], depth + 1)
            copied_key = self.copy(key, depth + 1)
            result[copied_key] = value
        return result

def _behavior_copy(value, limit=_BehaviorLimits.MAX_JSON_BYTES):
    result = _BehaviorCopier(limit).copy(value, 0)
    encoded = _behavior_json.dumps(result, ensure_ascii=False, allow_nan=False, separators=(',', ':')).encode('utf-8')
    if len(encoded) > limit:
        _behavior_invalid('Behavior JSON exceeds byte bounds.')
    return result

def _behavior_keys(value, allowed):
    if type(value) is not dict or any(key not in allowed for key in value):
        _behavior_invalid('Unknown or invalid behavior options.')

def _behavior_page_bounds_valid(page, size, policy):
    if type(page) is not int or type(size) is not int:
        return False
    if not 0 <= page <= _BehaviorLimits.MAX_PAGE:
        return False
    if not 1 <= size <= policy['maxSize']:
        return False
    return page * size <= _BehaviorLimits.MAX_PAGE_ROWS

def _behavior_page_sort(sort, policy):
    if type(sort) is not list or len(sort) > _BehaviorLimits.MAX_SORT_FIELDS:
        _behavior_invalid('Invalid behavior sort.')
    seen = set()
    normalized = []
    for item in sort:
        if type(item) is not str:
            _behavior_invalid('Invalid behavior sort.')
        parts = item.split(':')
        field = parts[0]
        direction = 'asc'
        if len(parts) == 2:
            direction = parts[1]
        if len(parts) > 2 or field not in policy['sort'] or direction not in ('asc', 'desc') or field in seen:
            _behavior_invalid('Invalid behavior sort.')
        seen.add(field)
        normalized.append(field + ':' + direction)
    return normalized

def _behavior_page(value, policy):
    _behavior_keys(value, ('page', 'size', 'sort', 'offset'))
    value = _behavior_copy(value, _BehaviorLimits.SMALL_JSON_BYTES)
    page = value.get('page', 0)
    size = value.get('size', policy['defaultSize'])
    if not _behavior_page_bounds_valid(page, size, policy):
        _behavior_invalid('Invalid behavior pagination bounds.')
    if 'offset' in value and (type(value['offset']) is not int or value['offset'] != page * size):
        _behavior_invalid('Invalid behavior pagination offset.')
    sort = _behavior_page_sort(value.get('sort', []), policy)
    return {'page': page, 'size': size, 'offset': page * size, 'sort': sort}

class BehaviorSignal:
    def __init__(self):
        self.aborted = False
        self._listeners = set()
    def abort(self):
        self.aborted = True
        for listener in tuple(self._listeners):
            listener()
    def _listen(self, callback):
        if len(self._listeners) >= _BehaviorLimits.MAX_SIGNAL_LISTENERS:
            _behavior_invalid('Behavior signal exceeds owner bounds.')
        self._listeners.add(callback)
        if self.aborted:
            callback()
        return lambda: self._listeners.discard(callback)

class _BehaviorExecution:
    def __init__(self, context):
        self.partition = context.partition
        self.environment = context.factory.environment
        self.signal = context.signal
        self.deadline = int(_behavior_time.time() * 1000) + context.timeout
        self._until = _behavior_time.monotonic() + context.timeout / 1000
    def check(self):
        if self.signal.aborted:
            raise BehaviorError('cancelled', 'Behavior operation was cancelled.')
        if _behavior_time.monotonic() >= self._until:
            raise BehaviorError('timeout', 'Behavior operation exceeded its deadline.')

async def _behavior_await(value, execution):
    try:
        execution.check()
    except BaseException:
        if _behavior_inspect.iscoroutine(value):
            value.close()
        elif _behavior_asyncio.isfuture(value):
            value.cancel()
        raise
    if not _behavior_inspect.isawaitable(value):
        return value
    task = _behavior_asyncio.ensure_future(value)
    remove = execution.signal._listen(task.cancel)
    try:
        remaining = max(0, execution._until - _behavior_time.monotonic())
        result = await _behavior_asyncio.wait_for(task, remaining)
        execution.check()
        return result
    except _behavior_asyncio.TimeoutError:
        raise BehaviorError('timeout', 'Behavior operation exceeded its deadline.') from None
    except _behavior_asyncio.CancelledError:
        raise BehaviorError('cancelled', 'Behavior operation was cancelled.') from None
    finally:
        remove()

async def _behavior_adapter(function, args, execution):
    execution.check()
    try:
        result = await _behavior_await(function(*args), execution)
        execution.check()
        return result
    except BehaviorError:
        raise
    except Exception:
        raise BehaviorError('adapter', 'Behavior adapter failed.') from None

def _behavior_safe_path(path):
    if type(path) is not str or not path.startswith('/') or path.startswith('//'):
        return False
    if len(path) > _BehaviorLimits.MAX_PATH_LENGTH:
        return False
    if any(char in path for char in '\\?#%') or any(ord(char) < 33 for char in path):
        return False
    return not any(part in ('.', '..') for part in path.split('/'))

def _behavior_read_path(value, source):
    current = value
    for name in source.split('.'):
        if isinstance(current, dict):
            current = current.get(name)
        else:
            current = None
    return current

def _behavior_map_value(policy, args, original=None):
    value = None
    if policy['argument'] < len(args):
        value = args[policy['argument']]
    typed = 'result' in policy
    if typed:
        value = _behavior_builtins.YonModels.plain(value)
    if not isinstance(value, dict):
        _behavior_invalid('A behavior mapping reads one JSON object argument.')
    output = _behavior_copy(_behavior_map_fields(value, policy['fields']))
    if not typed:
        return output
    result = _behavior_typing.get_type_hints(original).get('return')
    return _behavior_builtins.YonModels.read(result, output)

def _behavior_map_fields(value, fields):
    output = {}
    for field in fields:
        if 'source' not in field:
            output[field['target']] = field.get('constant')
            continue
        found = _behavior_read_path(value, field['source'])
        output[field['target']] = _behavior_shaped(found, field['fields']) if 'fields' in field else found
    return output

def _behavior_shaped(value, fields):
    if isinstance(value, list):
        return [_behavior_shaped(item, fields) for item in value]
    return _behavior_map_fields(value, fields) if isinstance(value, dict) else value

def _behavior_pick_argument(args, index):
    if index >= len(args):
        _behavior_invalid('A request argument is missing.')
    return args[index]

def _behavior_exchange_path(policy, args):
    path = policy['path']
    for name, index in (policy.get('pathVariables') or {}).items():
        value = _behavior_pick_argument(args, index)
        text = ('true' if value else 'false') if type(value) is bool else value
        if type(text) in (int, float):
            text = str(text)
        if type(text) is not str or not _behavior_re.fullmatch(r'[A-Za-z0-9._~-]{1,256}', text) or text in ('.', '..'):
            _behavior_invalid('A path variable may use only letters, digits and - _ . ~')
        path = path.replace('{' + name + '}', text)
    return path

def _behavior_exchange_request(policy, args):
    descriptor = {'query': {}, 'headers': {}}
    for name, index in (policy.get('query') or {}).items():
        descriptor['query'][name] = _behavior_pick_argument(args, index)
    for name, index in (policy.get('headers') or {}).items():
        descriptor['headers'][name] = _behavior_pick_argument(args, index)
    if 'body' in policy:
        descriptor['body'] = _behavior_pick_argument(args, policy['body'])
    return descriptor

def _behavior_retryable(error):
    return not isinstance(error, BehaviorError) or error.code == 'adapter'

def _behavior_backoff(policy, attempt):
    delay = min(policy['delayMs'] * policy['multiplier'] ** attempt, policy['maxDelayMs'])
    return round(delay + _behavior_random.random() * policy['jitterMs'])

def _behavior_environment_properties(values, environment):
    result = {}
    for path, value in _behavior_application_source(values).items():
        if path.startswith(environment + '.'):
            result[path.replace('.', '_').upper()] = value
    return result

def _behavior_valid_property(key, value, prefix):
    if not key.startswith(prefix):
        return False
    if not _behavior_re.fullmatch(_BehaviorText.PROPERTY_KEY, key):
        return False
    if type(value) is not str:
        return False
    return len(value.encode('utf-8')) <= _BehaviorLimits.MAX_TEXT_BYTES

def _behavior_properties_source(values, environment, code):
    try:
        values = _behavior_copy(values, _BehaviorLimits.CONFIGURATION_JSON_BYTES)
        if type(values) is dict and ('tac' in values or 'yon' in values):
            values = _behavior_environment_properties(values, environment)
        prefix = 'YON_'
        if environment == 'tac':
            prefix = 'TAC_'
        if type(values) is not dict or len(values) > _BehaviorLimits.MAX_PROPERTIES:
            raise ValueError()
        for key, value in values.items():
            if not _behavior_valid_property(key, value, prefix):
                raise ValueError()
        return values
    except Exception:
        raise BehaviorError(code, 'Invalid behavior configuration values.') from None

def _behavior_valid_profiles(profiles):
    if type(profiles) is not list or len(profiles) > _BehaviorLimits.MAX_PROFILES:
        return False
    for name in profiles:
        if type(name) is not str or not _behavior_re.fullmatch(_BehaviorText.PROFILE_NAME, name):
            return False
    return len(set(profiles)) == len(profiles)

def _behavior_condition_matches(condition, properties):
    name = condition['name']
    if any(key.startswith(name + '_') for key in properties):
        return False
    if name in properties:
        return properties[name] == condition['havingValue']
    return condition['matchIfMissing']

def _behavior_config_options(options, environment, record):
    _behavior_keys(options, _BehaviorText.CONTEXT_OPTIONS)
    profiles = options.get('profiles', [])
    if not _behavior_valid_profiles(profiles):
        raise BehaviorError('configuration', 'Invalid behavior profiles.')
    properties = _behavior_properties_source(options.get('properties', {}), environment, 'configuration')
    if 'profiles' in record and not set(record['profiles']).intersection(profiles):
        raise BehaviorError('inactive-configuration', 'Inactive behavior configuration.')
    condition = record.get('conditionalOnProperty')
    if condition and not _behavior_condition_matches(condition, properties):
        raise BehaviorError('inactive-configuration', 'Inactive behavior configuration.')
    return dict(options, profiles=list(profiles), properties=properties)

def _behavior_valid_port(value):
    if type(value) not in (int, float):
        return False
    if value != int(value):
        return False
    return abs(value) <= _BehaviorLimits.MAX_SAFE_INTEGER

def _behavior_property_text(value):
    if type(value) is str:
        return value
    if type(value) is float and value.is_integer():
        return str(int(value))
    return _behavior_json.dumps(value, allow_nan=False, separators=(',', ':'))

def _behavior_flatten(value, path, depth, flat):
    if depth > _BehaviorLimits.MAX_PROPERTY_DEPTH or len(path) > _BehaviorLimits.MAX_PROPERTY_PATH:
        raise ValueError()
    if len(flat) >= _BehaviorLimits.MAX_PROPERTIES:
        raise ValueError()
    if path == 'yon.server.port' and not _behavior_valid_port(value):
        raise ValueError()
    if type(value) is dict:
        if depth > 0 and not value:
            flat[path + '.'] = ''
        for key, child in value.items():
            if not _behavior_re.fullmatch(_BehaviorText.APPLICATION_KEY, key):
                raise ValueError()
            _behavior_flatten(child, path + '.' + key, depth + 1, flat)
        return
    if value is None or type(value) is list:
        flat[path + '.'] = ''
        return
    if type(value) not in (str, int, float, bool) or len(flat) >= _BehaviorLimits.MAX_PROPERTIES:
        raise ValueError()
    text = _behavior_property_text(value)
    if len(text.encode()) > _BehaviorLimits.MAX_TEXT_BYTES:
        raise ValueError()
    flat[path] = text

def _behavior_application_source(application):
    try:
        application = _behavior_copy(application, _BehaviorLimits.CONFIGURATION_JSON_BYTES)
        if type(application) is not dict or ('tac' not in application and 'yon' not in application):
            raise ValueError()
        flat = {}
        for root, value in application.items():
            if root not in ('tac', 'yon'):
                continue
            if type(value) is not dict:
                raise ValueError()
            _behavior_flatten(value, root, 0, flat)
        return flat
    except Exception:
        raise BehaviorError('configuration-properties', 'Invalid application configuration.') from None

def _behavior_parse_property(descriptor, values, name, kind):
    if name not in values:
        return descriptor['default']
    text = values[name]
    if kind == 'string':
        return text
    if kind == 'boolean':
        if text not in ('true', 'false'):
            raise ValueError()
        return text == 'true'
    grammar = _BehaviorText.NUMBER
    if kind == 'integer':
        grammar = _BehaviorText.INTEGER
    if not _behavior_re.fullmatch(grammar, text):
        raise ValueError()
    if kind == 'integer':
        return int(text)
    return float(text)

def _behavior_number_in_range(value, kind, descriptor):
    if type(value) not in (int, float):
        return False
    if not _behavior_math.isfinite(value) or abs(value) > _BehaviorLimits.MAX_SAFE_INTEGER:
        return False
    if kind == 'integer' and value != int(value):
        return False
    if 'min' in descriptor and value < descriptor['min']:
        return False
    if 'max' in descriptor and value > descriptor['max']:
        return False
    return True

def _behavior_property_value(descriptor, values, name):
    try:
        kind = descriptor['type']
        if any(path.startswith(name + '.') for path in values):
            raise ValueError()
        value = _behavior_parse_property(descriptor, values, name, kind)
        if kind == 'string' and (type(value) is not str or len(value.encode('utf-8')) > _BehaviorLimits.MAX_TEXT_BYTES):
            raise ValueError()
        if kind in ('integer', 'number') and not _behavior_number_in_range(value, kind, descriptor):
            raise ValueError()
        return value
    except Exception:
        raise BehaviorError('configuration-properties', 'Invalid or missing typed configuration property.') from None

def _behavior_validation_failure(path):
    raise BehaviorError('validation', 'Behavior validation failed.', path)

def _behavior_size(data):
    if type(data) is str:
        return len(data.encode('utf-8'))
    if type(data) in (list, dict):
        return len(data)
    return -1

def _behavior_validate_field(field, rules, data):
    if rules.get('notNull') and data is None:
        _behavior_validation_failure(field + '.notNull')
    if rules.get('notBlank') and (type(data) is not str or not data.strip(_BehaviorText.WHITESPACE)):
        _behavior_validation_failure(field + '.notBlank')
    if data is None:
        return
    if 'size' in rules:
        size = _behavior_size(data)
        if not rules['size']['min'] <= size <= rules['size']['max']:
            _behavior_validation_failure(field + '.size')
    if 'min' in rules and (type(data) not in (int, float) or data < rules['min']):
        _behavior_validation_failure(field + '.min')
    if 'max' in rules and (type(data) not in (int, float) or data > rules['max']):
        _behavior_validation_failure(field + '.max')

def _behavior_validate(schema, candidate):
    try:
        value = _behavior_copy(candidate)
    except Exception:
        _behavior_validation_failure('$.json')
    if type(value) is not dict:
        _behavior_validation_failure('$.object')
    if type(schema) is not dict or type(schema.get('fields')) is not dict:
        _behavior_validation_failure('$.schema')
    for field, rules in schema['fields'].items():
        _behavior_validate_field(field, rules, value.get(field))
    return value

def _behavior_validate_argument(args, policy):
    if policy['argument'] >= len(args):
        _behavior_validation_failure('$.json')
    args[policy['argument']] = _behavior_validate(policy['schema'], args[policy['argument']])

def _behavior_check_partition(partition):
    if type(partition) is not str or not partition.strip():
        _behavior_invalid('Behavior partition is required and bounded.')
    if len(partition) > _BehaviorLimits.MAX_PARTITION_BYTES:
        _behavior_invalid('Behavior partition is required and bounded.')
    try:
        partition_bytes = len(partition.encode('utf-8'))
    except UnicodeEncodeError:
        _behavior_invalid('Behavior partition requires valid UTF-8.')
    if partition_bytes > _BehaviorLimits.MAX_PARTITION_BYTES:
        _behavior_invalid('Behavior partition is required and bounded.')

def _behavior_fresh_cache_hit(hit, policy):
    _behavior_keys(hit, ('hit', 'value', 'expiresAt'))
    if type(hit.get('hit')) is not bool:
        _behavior_invalid('Invalid cache response.')
    if not hit['hit']:
        return False
    expiry = hit.get('expiresAt')
    now = int(_behavior_time.time() * 1000)
    if type(expiry) is not int or 'value' not in hit or expiry > now + policy['ttlMs']:
        _behavior_invalid('Invalid cache expiry.')
    return expiry > now

def _behavior_mark_cache_expiry(execution, policy):
    execution.policy = dict(policy)
    execution.expiresAt = int(_behavior_time.time() * 1000) + policy['ttlMs']

def _behavior_is_scalar(item):
    return item is None or type(item) in (str, bool, int, float)

def _behavior_check_query_arity(policy, args):
    indices = [predicate['argument'] for predicate in policy['where'] if 'argument' in predicate]
    if policy['pageArgument'] is not None:
        indices.append(policy['pageArgument'])
    if len(args) != max(indices + [-1]) + 1:
        _behavior_invalid('Query requires its exact declared positional arguments.')

def _behavior_query_binding(predicate, args):
    if predicate['argument'] >= len(args):
        _behavior_invalid('Query argument is missing.')
    value = args[predicate['argument']]
    if predicate['operator'] == 'in':
        valid = type(value) is list and len(value) <= _BehaviorLimits.MAX_IN_VALUES and all(_behavior_is_scalar(item) for item in value)
    else:
        valid = _behavior_is_scalar(value)
    if not valid:
        _behavior_invalid('Query requires finite scalar bindings or a bounded In array.')
    return value

def _behavior_query_where(policy, args):
    where = []
    for predicate in policy['where']:
        entry = {'field': predicate['field'], 'operator': predicate['operator']}
        if 'argument' in predicate:
            entry['value'] = _behavior_query_binding(predicate, args)
        where.append(entry)
    return where

def _behavior_query_page(policy, args):
    index = policy['pageArgument']
    if index is not None and index >= len(args):
        _behavior_invalid('Query page argument is missing.')
    requested = {}
    if index is not None:
        requested = args[index]
    page_policy = {'defaultSize': policy['defaultSize'], 'maxSize': policy['maxSize'], 'sort': policy['fields']}
    page = _behavior_page(requested, page_policy)
    if page['sort']:
        _behavior_invalid('Query ordering is annotation-owned.')
    return page

def _behavior_valid_rows(result, size):
    if type(result) is not list or len(result) > size:
        return False
    return all(type(row) is dict for row in result)

def _behavior_check_query_result(policy, query, result):
    operation = policy['operation']
    if operation in ('find', 'findOne') and not _behavior_valid_rows(result, query['page']['size']):
        _behavior_invalid('Query find result requires a bounded array of objects.')
    if operation in ('count', 'delete') and (type(result) is not int or not 0 <= result <= _BehaviorLimits.MAX_SAFE_INTEGER):
        _behavior_invalid('Query count and delete results require a nonnegative safe integer.')
    if operation == 'exists' and type(result) is not bool:
        _behavior_invalid('Query exists result requires a boolean.')

def _behavior_check_query_values(query):
    for value in query.values():
        if type(value) not in (str, int, float, bool):
            _behavior_invalid('Invalid HTTP query.')

def _behavior_valid_header(key, value):
    if not 1 <= len(key) <= _BehaviorLimits.MAX_HEADER_NAME:
        return False
    if any(char not in _BehaviorText.HEADER_TOKEN for char in key):
        return False
    if type(value) is not str:
        return False
    return not any(char in value for char in '\r\n\x00')

def _behavior_check_headers(headers):
    for key, value in headers.items():
        if not _behavior_valid_header(key, value):
            _behavior_invalid('Invalid HTTP header.')
        if key.lower() in _BehaviorText.TRANSPORT_HEADERS:
            _behavior_invalid('HTTP header is transport-owned.')

class _BehaviorCall:
    def __init__(self, metadata, instance, args, original, execution):
        self.metadata = metadata
        self.instance = instance
        self.args = args
        self.original = original
        self.execution = execution

class _BehaviorContext:
    def __init__(self, factory, options):
        _behavior_keys(options, _BehaviorText.CONTEXT_OPTIONS)
        self.factory = factory
        self.partition = options.get('partition')
        _behavior_check_partition(self.partition)
        owner = options.get('signal')
        if not isinstance(owner, BehaviorSignal):
            _behavior_invalid('BehaviorSignal is required.')
        self.signal = BehaviorSignal()
        self._remove = owner._listen(self.dispose)
        self.timeout = options.get('timeoutMs', _BehaviorLimits.DEFAULT_TIMEOUT_MS)
        if type(self.timeout) is not int or not 1 <= self.timeout <= _BehaviorLimits.MAX_TIMEOUT_MS:
            self._remove()
            _behavior_invalid('Invalid behavior timeout.')
        self.adapters = {}
        self.active = 0
        self._slots = {}
        self.setup_key = None
    def dispose(self):
        self.signal.abort()
        if hasattr(self, '_remove'):
            self._remove()
        setup_key = getattr(self, 'setup_key', None)
        if setup_key is not None:
            self.setup_key = None
            _BehaviorFactory.release_setup(setup_key)
    def _begin(self):
        execution = _BehaviorExecution(self)
        execution.check()
        if self.active >= _BehaviorLimits.MAX_ACTIVE_CALLS:
            _behavior_invalid('Behavior context exceeds concurrency bounds.')
        self.active += 1
        return execution
    def _builtin(self, group, name):
        configured = self.adapters.get(group, {}).get(name)
        return configured is None and _BehaviorHostAdapters.find(self.factory.environment, group, name) is not None
    def _adapter(self, group, name, operation):
        adapter = self.adapters.get(group, {}).get(name)
        if adapter is None:
            adapter = _BehaviorHostAdapters.find(self.factory.environment, group, name)
        if type(adapter) is dict:
            function = adapter.get(operation)
        else:
            function = getattr(adapter, operation, None)
        if not callable(function):
            _behavior_invalid('Required behavior adapter is missing.')
        return function
    def _record(self, instance):
        record = self.factory.records.get(type(instance))
        if not record or getattr(instance, 'behaviors', None) is not self:
            _behavior_invalid('Behavior instance requires its source-local context.')
        return record
    def _arguments(self, args, metadata):
        args = list(args)
        if len(args) > _BehaviorLimits.MAX_ARGUMENTS:
            _behavior_invalid('Too many behavior arguments.')
        if 'valid' in metadata:
            _behavior_validate_argument(args, metadata['valid'])
        if 'pageable' in metadata:
            policy = metadata['pageable']
            index = policy['argument']
            if index >= len(args):
                _behavior_invalid('Missing pagination argument.')
            args[index] = _behavior_page(args[index], policy)
        return args
    async def _key(self, record, name, args, execution, operation):
        metadata = record['methods'][name]
        key_arguments = metadata['cacheable']['keyArgs']
        if any(index >= len(args) for index in key_arguments):
            _behavior_invalid('Cache key argument is missing.')
        selected = _behavior_copy([args[index] for index in key_arguments], _BehaviorLimits.SMALL_JSON_BYTES)
        call = {'operation': operation, 'cache': record['cache']['name'], 'type': record['name'], 'method': name, 'keyArguments': _behavior_copy(selected)}
        authorize = self.adapters.get('authorize')
        builtin = authorize is None and self._builtin('caches', record['cache']['name'])
        if not builtin and not callable(authorize):
            raise BehaviorError('unauthorized', 'Behavior cache access denied.')
        if not builtin and await _behavior_adapter(authorize, (call, execution), execution) is not True:
            raise BehaviorError('unauthorized', 'Behavior cache access denied.')
        key = [self.partition, record['cache']['name'], self.factory.module_id, record['name'], name, selected]
        key = _behavior_copy(key, _BehaviorLimits.SMALL_JSON_BYTES)
        return _behavior_json.dumps(key, ensure_ascii=False, separators=(',', ':'))
    async def invoke(self, instance, name, args, original):
        record = self._record(instance)
        metadata = record['methods'][name]
        execution = self._begin()
        try:
            args = self._arguments(args, metadata)
            if 'query' in metadata:
                return await self._query(metadata['query'], args, execution)
            if 'cachePut' in metadata or 'cacheEvict' in metadata:
                return await self._mutate(record, name, metadata, instance, args, original, execution)
            if 'cacheable' in metadata:
                return await self._cached_invoke(record, name, metadata, instance, args, original, execution)
            execution.check()
            result = await self._resilient(record, name, metadata, instance, args, original, execution)
            result = _behavior_copy(result)
            execution.check()
            return result
        finally:
            self.active -= 1
    async def _cached_invoke(self, record, name, metadata, instance, args, original, execution):
        policy = record['cache']
        key = await self._key(record, name, args, execution, 'cache:read')
        read = self._adapter('caches', policy['name'], 'get')
        hit = await _behavior_adapter(read, (key, execution), execution)
        if _behavior_fresh_cache_hit(hit, policy):
            result = _behavior_copy(hit['value'], policy['maxBytes'])
            execution.check()
            return result
        execution.check()
        result = await self._resilient(record, name, metadata, instance, args, original, execution)
        result = _behavior_copy(result)
        result = _behavior_copy(result, policy['maxBytes'])
        _behavior_mark_cache_expiry(execution, policy)
        store = self._adapter('caches', policy['name'], 'set')
        await _behavior_adapter(store, (key, _behavior_copy(result), execution), execution)
        execution.check()
        return result
    async def _resilient(self, record, name, metadata, instance, args, original, execution):
        call = _BehaviorCall(metadata, instance, args, original, execution)
        limit = metadata.get('concurrencyLimit')
        if limit is None:
            return await self._retried(call)
        return await self._limited(record['name'] + '.' + name, limit, lambda: self._retried(call), execution)
    async def _run_target(self, call):
        metadata = call.metadata
        if metadata.get('mapping') is not None:
            return _behavior_map_value(metadata['mapping'], call.args, call.original)
        policy = metadata.get('httpExchange')
        if policy is not None and policy.get('generated'):
            filled = dict(policy, path=_behavior_exchange_path(policy, call.args))
            return await self._exchange(filled, _behavior_exchange_request(policy, call.args), call.execution, True)
        result = await _behavior_await(call.original(call.instance, *call.args), call.execution)
        if policy is not None:
            result = await self._exchange(policy, result, call.execution)
        return result
    async def _transaction_control(self, adapter, action, identifier, execution):
        command = {'operation': 'transaction', 'action': action}
        if identifier is not None:
            command['transaction'] = identifier
        return await _behavior_adapter(adapter, (command, execution), execution)
    async def _begin_transaction(self, adapter, execution):
        begun = await self._transaction_control(adapter, 'begin', None, execution)
        begun = _behavior_copy(begun, _BehaviorLimits.SMALL_JSON_BYTES)
        identifier = None
        if type(begun) is dict:
            identifier = begun.get('transaction')
        if type(identifier) is not str or not 0 < len(identifier) <= _BehaviorLimits.MAX_TRANSACTION_ID:
            _behavior_invalid('A transactional adapter must answer a bounded transaction identifier.')
        return identifier
    async def _transacted(self, call):
        policy = call.metadata.get('transactional')
        if policy is None or _BehaviorOpenTransactions.of(policy['adapter']) is not None:
            return await self._run_target(call)
        adapter = self._adapter('entities', policy['adapter'], 'execute')
        identifier = await self._begin_transaction(adapter, call.execution)
        try:
            result = await _BehaviorOpenTransactions.within(policy['adapter'], identifier, lambda: self._run_target(call))
        except BaseException:
            try:
                await self._transaction_control(adapter, 'rollback', identifier, call.execution)
            except Exception:
                pass
            raise
        await self._transaction_control(adapter, 'commit', identifier, call.execution)
        return result
    async def _retried(self, call):
        policy = call.metadata.get('retryable')
        if policy is None:
            return await self._transacted(call)
        tries = 0
        while True:
            try:
                return await self._transacted(call)
            except Exception as error:
                if tries >= policy['maxRetries'] or not _behavior_retryable(error):
                    raise
                wait = _behavior_backoff(policy, tries)
                tries += 1
                if wait > 0:
                    await _behavior_await(_behavior_asyncio.sleep(wait / 1000), call.execution)
    async def _limited(self, key, policy, attempt, execution):
        slot = self._slots.get(key)
        if slot is None:
            slot = {'active': 0, 'waiters': []}
            self._slots[key] = slot
        if slot['active'] < policy['limit']:
            slot['active'] += 1
        elif policy['policy'] == 'reject':
            raise BehaviorError('capacity', 'Behavior method concurrency limit reached.')
        else:
            await self._wait_for_slot(key, slot, execution)
        try:
            return await attempt()
        finally:
            self._release(key, slot)
    async def _wait_for_slot(self, key, slot, execution):
        waiter = _behavior_asyncio.get_running_loop().create_future()
        slot['waiters'].append(waiter)
        try:
            await _behavior_await(waiter, execution)
        except Exception:
            if waiter in slot['waiters']:
                slot['waiters'].remove(waiter)
            else:
                self._release(key, slot)
            raise
    def _release(self, key, slot):
        while slot['waiters']:
            waiter = slot['waiters'].pop(0)
            if not waiter.done():
                waiter.set_result(True)
                return
        slot['active'] -= 1
        if slot['active'] <= 0:
            self._slots.pop(key, None)
    async def _mutate(self, record, name, metadata, instance, args, original, execution):
        put = 'cachePut' in metadata
        if put:
            mutation = metadata['cachePut']
            adapter_operation = 'set'
            key_operation = 'cache:put'
        else:
            mutation = metadata['cacheEvict']
            adapter_operation = 'delete'
            key_operation = 'cache:evict'
        if any(index >= len(args) for index in mutation['arguments']):
            _behavior_invalid('Cache mutation argument is missing.')
        target = record['methods'][mutation['method']]
        page_policy = {}
        if 'pageable' in target:
            page_policy = {'pageable': target['pageable']}
        mapped = self._arguments([args[index] for index in mutation['arguments']], page_policy)
        policy = record['cache']
        adapter = self._adapter('caches', policy['name'], adapter_operation)
        key = await self._key(record, mutation['method'], mapped, execution, key_operation)
        result = await self._resilient(record, name, metadata, instance, args, original, execution)
        limit = _BehaviorLimits.MAX_JSON_BYTES
        if put:
            limit = policy['maxBytes']
        result = _behavior_copy(result, limit)
        execution.check()
        if put:
            _behavior_mark_cache_expiry(execution, policy)
            await _behavior_adapter(adapter, (key, _behavior_copy(result), execution), execution)
        else:
            await _behavior_adapter(adapter, (key, execution), execution)
        execution.check()
        return result
    async def _exchange(self, policy, descriptor, execution, browser=False):
        _behavior_keys(descriptor, ('query', 'headers', 'body'))
        descriptor = _behavior_copy(descriptor)
        query = descriptor.get('query', {})
        headers = descriptor.get('headers', {})
        if type(query) is not dict or type(headers) is not dict:
            _behavior_invalid('Invalid HTTP descriptor.')
        _behavior_check_query_values(query)
        _behavior_check_headers(headers)
        if policy['method'] in _BehaviorText.READ_METHODS and 'body' in descriptor:
            _behavior_invalid('HTTP reads cannot contain a request body.')
        request = dict(descriptor, method=policy['method'], path=policy['path'], query=query, headers=headers,
                       redirect='error', credentials='omit', maxResponseBytes=_BehaviorLimits.MAX_JSON_BYTES,
                       signal=execution.signal, deadline=execution.deadline)
        custom = self.adapters.get('http', {}).get(policy['client'])
        if browser and custom is None and _BehaviorBrowserHttp.available(self.factory.environment):
            return _behavior_copy(await _BehaviorBrowserHttp.exchange(request, execution))
        exchange = self._adapter('http', policy['client'], 'exchange')
        return _behavior_copy(await _behavior_adapter(exchange, (request, execution), execution))
    async def _query(self, policy, args, execution):
        _behavior_check_query_arity(policy, args)
        where = _behavior_query_where(policy, args)
        single = policy['operation'] == 'findOne'
        query = {'kind': 'find' if single else policy['operation'], 'where': _behavior_copy(where, _BehaviorLimits.CONFIGURATION_JSON_BYTES), 'orderBy': _behavior_copy(policy['orderBy'])}
        if policy['operation'] == 'find':
            query['page'] = _behavior_query_page(policy, args)
        if single:
            query['page'] = _behavior_page({'size': 1}, {'defaultSize': 1, 'maxSize': 1, 'sort': policy['fields']})
        entity = policy['entity']
        command = {'operation': 'query', 'entity': entity['name'], 'table': entity['table'], 'idField': entity['id'], 'query': query}
        joined = _BehaviorOpenTransactions.of(entity['adapter'])
        if joined is not None:
            command['transaction'] = joined
        authorize = self.adapters.get('authorizeEntity')
        builtin = authorize is None and self._builtin('entities', entity['adapter'])
        if not builtin and not callable(authorize):
            _behavior_invalid('Store query requires entity authorization.')
        adapter = self._adapter('entities', entity['adapter'], 'execute')
        if not builtin and await _behavior_adapter(authorize, (_behavior_copy(command), execution), execution) is not True:
            raise BehaviorError('unauthorized', 'Store query was not authorized.')
        result = _behavior_copy(await _behavior_adapter(adapter, (_behavior_copy(command), execution), execution))
        _behavior_check_query_result(policy, query, result)
        execution.check()
        if single:
            return result[0] if result else None
        return result
    async def evict(self, instance, name, args):
        record = self._record(instance)
        metadata = record.get('methods', {}).get(name, {})
        if 'cacheable' not in metadata:
            _behavior_invalid('Eviction requires a cacheable method.')
        execution = self._begin()
        try:
            args = self._arguments(args, metadata)
            key = await self._key(record, name, args, execution, 'cache:evict')
            delete = self._adapter('caches', record['cache']['name'], 'delete')
            await _behavior_adapter(delete, (key, execution), execution)
        finally:
            self.active -= 1
    def rest_client(self, name):
        if not _behavior_re.fullmatch(_BehaviorText.IDENTIFIER, name or ''):
            _behavior_invalid('RestClient requires a configured http adapter name.')
        return _BehaviorRestClient(self, name)
    def repository(self, entity):
        record = self.factory.records.get(entity, {})
        if 'entity' not in record:
            _behavior_invalid('Store requires a source-local Entity/Table.')
        return _BehaviorRepository(self, record)

class _BehaviorHostCache:
    def __init__(self, name):
        self.name = name
    def get(self, key, execution):
        return _BehaviorHostAdapters.ask({'op': 'cache.get', 'cache': self.name, 'key': key})
    def set(self, key, value, execution):
        return _BehaviorHostAdapters.ask({'op': 'cache.set', 'cache': self.name, 'key': key, 'value': value,
                                          'expiresAt': execution.expiresAt, 'maxEntries': execution.policy['maxEntries']})
    def delete(self, key, execution):
        return _BehaviorHostAdapters.ask({'op': 'cache.delete', 'cache': self.name, 'key': key})

class _BehaviorHostEntities:
    def execute(self, command, execution):
        return _BehaviorHostAdapters.ask({'op': 'entity', 'command': command})

class _BehaviorHostAdapters:
    ENTITIES = 'indexeddb'
    @staticmethod
    def find(environment, group, name):
        if environment != 'tac' or not callable(getattr(_behavior_builtins, 'TacStorage', None)):
            return None
        if group == 'caches':
            return _BehaviorHostCache(name)
        if group == 'entities' and name == _BehaviorHostAdapters.ENTITIES:
            return _BehaviorHostEntities()
        return None
    @staticmethod
    def ask(request):
        answer = _behavior_json.loads(str(_behavior_builtins.TacStorage(_behavior_json.dumps(request, allow_nan=False))))
        if type(answer) is not dict:
            raise BehaviorError('storage', 'Tachyon storage did not answer.')
        if type(answer.get('error')) is str:
            raise BehaviorError('storage', answer['error'])
        return answer.get('value')

class _BehaviorBrowserHttp:
    KEYS = ('method', 'path', 'query', 'headers', 'body', 'redirect', 'credentials', 'maxResponseBytes')
    CODES = ('HTTP_REQUEST', 'HTTP_ORIGIN', 'HTTP_HEADER', 'HTTP_QUERY', 'HTTP_BODY', 'HTTP_LIMIT', 'HTTP_RESPONSE',
             'HTTP_STATUS', 'HTTP_CAPACITY', 'HTTP_NETWORK', 'HTTP_MISSING', 'CANCELLED', 'DEADLINE')
    @staticmethod
    def available(environment):
        return environment == 'tac' and callable(getattr(_behavior_builtins, 'TacHttp', None))
    @staticmethod
    async def exchange(request, execution):
        browser = {key: request[key] for key in _BehaviorBrowserHttp.KEYS if key in request}
        browser['deadline'] = execution.deadline
        execution.check()
        ticket = _behavior_builtins.TacHttp(_behavior_json.dumps(browser, allow_nan=False))
        remove = execution.signal._listen(lambda: ticket.cancel())
        try:
            text = await ticket.answer
        finally:
            remove()
        execution.check()
        return _BehaviorBrowserHttp.value_of(_behavior_json.loads(str(text)))
    @staticmethod
    def value_of(answer):
        if type(answer) is not dict:
            raise _BehaviorBrowserHttp.failure('HTTP_RESPONSE')
        if 'error' in answer:
            error = answer['error']
            code = error.get('code') if type(error) is dict else None
            raise _BehaviorBrowserHttp.failure(code if code in _BehaviorBrowserHttp.CODES else 'HTTP_NETWORK')
        if 'value' not in answer:
            raise _BehaviorBrowserHttp.failure('HTTP_RESPONSE')
        return answer['value']
    @staticmethod
    def failure(code):
        return BehaviorError(code, 'The browser HTTP request failed (' + code + ').')

class _BehaviorRestClient:
    def __init__(self, context, name):
        self.context = context
        self.name = name
    def get(self, path):
        return _BehaviorRestCall(self.context, self.name, 'GET', path)
    def post(self, path):
        return _BehaviorRestCall(self.context, self.name, 'POST', path)
    def put(self, path):
        return _BehaviorRestCall(self.context, self.name, 'PUT', path)
    def patch(self, path):
        return _BehaviorRestCall(self.context, self.name, 'PATCH', path)
    def delete(self, path):
        return _BehaviorRestCall(self.context, self.name, 'DELETE', path)
    def head(self, path):
        return _BehaviorRestCall(self.context, self.name, 'HEAD', path)
    def options(self, path):
        return _BehaviorRestCall(self.context, self.name, 'OPTIONS', path)

class _BehaviorRestCall:
    def __init__(self, context, name, method, path):
        if not _behavior_safe_path(path):
            _behavior_invalid('RestClient requires a bounded origin-relative path without query, fragment or backslash.')
        self.context = context
        self.name = name
        self.method = method
        self.path = path
        self.descriptor = {'query': {}, 'headers': {}}
    def query(self, values):
        self.descriptor['query'].update(_behavior_copy(values or {}, _BehaviorLimits.SMALL_JSON_BYTES))
        return self
    def header(self, key, value):
        self.descriptor['headers'][key] = value
        return self
    def body(self, value):
        if self.method in _BehaviorText.READ_METHODS:
            _behavior_invalid('HTTP reads cannot contain a request body.')
        self.descriptor['body'] = value
        return self
    async def retrieve(self):
        execution = self.context._begin()
        try:
            policy = {'client': self.name, 'method': self.method, 'path': self.path}
            return _behavior_copy(await self.context._exchange(policy, self.descriptor, execution))
        finally:
            self.context.active -= 1

class _BehaviorRepository:
    def __init__(self, context, record):
        self.context = context
        self.record = record
    async def _execute(self, operation, value):
        if operation not in ('find', 'save', 'delete'):
            _behavior_invalid('Unknown entity operation.')
        context = self.context
        record = self.record
        execution = context._begin()
        try:
            command = {'operation': operation, 'entity': record['name'], 'table': record['table'], 'idField': record['entity']['id']}
            copied = _behavior_copy(value)
            if operation == 'find':
                command['page'] = copied
            elif operation == 'save':
                command['value'] = copied
            else:
                command['id'] = copied
            joined = _BehaviorOpenTransactions.of(record['entity']['adapter'])
            if joined is not None:
                command['transaction'] = joined
            function = context._adapter('entities', record['entity']['adapter'], 'execute')
            return _behavior_copy(await _behavior_adapter(function, (command, execution), execution))
        finally:
            context.active -= 1
    async def find(self, page=None):
        requested = {}
        if page is not None:
            requested = page
        page_policy = {'defaultSize': _BehaviorLimits.DEFAULT_PAGE_SIZE, 'maxSize': _BehaviorLimits.MAX_REPOSITORY_PAGE_SIZE, 'sort': [self.record['entity']['id']]}
        return await self._execute('find', _behavior_page(requested, page_policy))
    async def save(self, value):
        allowed = ()
        if type(value) is dict:
            allowed = tuple(value)
        _behavior_keys(value, allowed)
        _behavior_entity_id(value.get(self.record['entity']['id']))
        return await self._execute('save', value)
    async def delete(self, identifier):
        _behavior_entity_id(identifier)
        return await self._execute('delete', identifier)

def _behavior_valid_entity_id(identifier):
    if type(identifier) is str:
        return 0 < len(identifier.encode('utf-8')) <= _BehaviorLimits.MAX_ENTITY_ID_BYTES
    if type(identifier) is int:
        return abs(identifier) <= _BehaviorLimits.MAX_SAFE_INTEGER
    return False

def _behavior_entity_id(identifier):
    _behavior_copy(identifier, _BehaviorLimits.ENTITY_ID_JSON_BYTES)
    if not _behavior_valid_entity_id(identifier):
        _behavior_invalid('Invalid entity identifier.')

def _behavior_validating_wrapper(metadata, original):
    def validated(instance, *args):
        args = list(args)
        _behavior_validate_argument(args, metadata['valid'])
        return original(instance, *args)
    if not _behavior_inspect.iscoroutinefunction(original):
        return validated
    async def asynchronous(instance, *args):
        return await validated(instance, *args)
    return asynchronous

class _BehaviorGateway:
    KINDS = ('invalid', 'timeout', 'capacity', 'adapter')
    MESSAGE = 'Gateway call failed.'

def _behavior_gateway_call(policy, args):
    if len(args) != policy['arity']:
        raise BehaviorError('invalid', _BehaviorGateway.MESSAGE)
    transport = getattr(_behavior_builtins, 'YonGateway', None)
    if transport is None:
        raise BehaviorError('adapter', _BehaviorGateway.MESSAGE)
    answer = transport.call(policy['key'], list(args))
    if type(answer) is dict and 'result' in answer:
        return answer['result']
    kind = answer.get('error') if type(answer) is dict else None
    if kind not in _BehaviorGateway.KINDS:
        kind = 'adapter'
    raise BehaviorError(kind, _BehaviorGateway.MESSAGE)

def _behavior_gateway_wrapper(policy, original):
    def call(instance, *args):
        return _behavior_gateway_call(policy, args)
    if not _behavior_inspect.iscoroutinefunction(original):
        return call
    async def asynchronous(instance, *args):
        return _behavior_gateway_call(policy, args)
    return asynchronous

def _behavior_mapping_wrapper(policy, original):
    def call(instance, *args):
        return _behavior_map_value(policy, args, original)
    if not _behavior_inspect.iscoroutinefunction(original):
        return call
    async def asynchronous(instance, *args):
        return _behavior_map_value(policy, args, original)
    return asynchronous

def _behavior_valid_adapter_name(name):
    if type(name) is not str:
        return False
    if not _behavior_re.fullmatch(_BehaviorText.IDENTIFIER, name):
        return False
    return name not in _BehaviorText.UNSAFE_ADAPTER_NAMES

def _behavior_check_adapter_collections(adapters):
    for group in _BehaviorText.ADAPTER_GROUPS:
        collection = adapters.get(group, {})
        if type(collection) is not dict or len(collection) > _BehaviorLimits.MAX_ADAPTERS:
            _behavior_invalid('Behavior adapter collections must be dictionaries.')
        for name in collection:
            if not _behavior_valid_adapter_name(name):
                _behavior_invalid('Invalid behavior adapter name.')

def _behavior_context_adapters(adapters):
    result = {}
    for group in _BehaviorText.ADAPTER_GROUPS:
        result[group] = dict(adapters.get(group, {}))
    if 'authorize' in adapters:
        result['authorize'] = adapters['authorize']
    if 'authorizeEntity' in adapters:
        if not callable(adapters['authorizeEntity']):
            _behavior_invalid('Entity authorize must be callable.')
        result['authorizeEntity'] = adapters['authorizeEntity']
    return result

class _BehaviorFactory:
    def __init__(self, records, environment, module_id):
        self.records = records
        self.environment = environment
        self.module_id = module_id
        for cls, record in records.items():
            for name, metadata in record['methods'].items():
                original = vars(cls).get(name)
                if not callable(original):
                    _behavior_invalid('Behavior target must be a concrete instance method.')
                setattr(cls, name, self._wrap(name, metadata, original))
    def _owned_context(self, instance):
        context = getattr(instance, 'behaviors', None)
        if not isinstance(context, _BehaviorContext) or context.factory is not self:
            _behavior_invalid('Behavior method requires its source-local context.')
        return context
    def _invoking_wrapper(self, name, original):
        async def wrapped(instance, *args):
            context = self._owned_context(instance)
            return await context.invoke(instance, name, args, original)
        return wrapped
    def _checking_wrapper(self, metadata, original):
        def wrapped(instance, *args):
            context = self._owned_context(instance)
            execution = context._begin()
            try:
                result = original(instance, *context._arguments(args, metadata))
                execution.check()
                return result
            finally:
                context.active -= 1
        return wrapped
    def _wrap(self, name, metadata, original):
        if set(metadata) == {'gateway'}:
            return _behavior_gateway_wrapper(metadata['gateway'], original)
        if set(metadata) == {'valid'}:
            return _behavior_validating_wrapper(metadata, original)
        if set(metadata) == {'mapping'}:
            return _behavior_mapping_wrapper(metadata['mapping'], original)
        if _behavior_inspect.iscoroutinefunction(original):
            return self._invoking_wrapper(name, original)
        return self._checking_wrapper(metadata, original)
    setups = {}
    async def create(self, configuration, options):
        if not self.records.get(configuration, {}).get('configuration'):
            _behavior_invalid('Setup must be declared in this source.')
        options = _behavior_config_options(options, self.environment, self.records[configuration])
        context = _BehaviorContext(self, options)
        try:
            setup_key = (configuration, tuple(sorted(options['profiles'])), tuple(sorted(options['properties'].items())))
            shared = _BehaviorFactory.setups.get(setup_key)
            if shared is None:
                shared = {'adapters': _behavior_asyncio.ensure_future(self._configured_adapters(configuration, context)), 'users': 0}
                _BehaviorFactory.setups[setup_key] = shared
            shared['users'] += 1
            context.setup_key = setup_key
            context.adapters = await shared['adapters']
            return context
        except BaseException:
            context.dispose()
            raise
    async def _configured_adapters(self, configuration, context):
        execution = _BehaviorExecution(context)
        adapters = await _behavior_adapter(configuration.configure, (execution,), execution)
        _behavior_keys(adapters, _BehaviorText.ADAPTER_OPTIONS)
        _behavior_check_adapter_collections(adapters)
        if 'authorize' in adapters and not callable(adapters['authorize']):
            _behavior_invalid('Behavior authorize must be callable.')
        return _behavior_context_adapters(adapters)
    @staticmethod
    def release_setup(setup_key):
        shared = _BehaviorFactory.setups.get(setup_key)
        if shared is None:
            return
        shared['users'] -= 1
        if shared['users'] == 0:
            del _BehaviorFactory.setups[setup_key]
    def construct(self, cls, values):
        record = self.records.get(cls, {})
        fields = record.get('constructor')
        if fields is None or type(values) is not dict or set(values) != set(fields) or any(values[field] is None for field in fields):
            _behavior_invalid('Constructor fields must exactly match their declaration.')
        return cls(*(values[field] for field in fields))
    def properties(self, cls, values):
        policy = self.records.get(cls, {}).get('configurationProperties')
        if not policy:
            raise BehaviorError('configuration-properties', 'Unknown configuration properties schema.')
        values = _behavior_properties_source(values, self.environment, 'configuration-properties')
        prefix = policy['prefix']
        names = {prefix + field['name'] for field in policy['fields'].values()}
        if any(key.startswith(prefix) and key not in names for key in values):
            raise BehaviorError('configuration-properties', 'Unknown configuration property.')
        result = {}
        for field, descriptor in policy['fields'].items():
            result[field] = _behavior_property_value(descriptor, values, prefix + descriptor['name'])
        return _behavior_copy(result, _BehaviorLimits.CONFIGURATION_JSON_BYTES)
    def values(self, cls, application):
        policy = self.records.get(cls, {}).get('values')
        if not policy:
            raise BehaviorError('configuration-properties', 'Unknown value bindings.')
        flat = _behavior_application_source(application)
        result = {}
        for field, descriptor in policy.items():
            result[field] = _behavior_property_value(descriptor, flat, descriptor['path'])
        return _behavior_copy(result, _BehaviorLimits.CONFIGURATION_JSON_BYTES)
    def validate(self, cls, value):
        return _behavior_validate(self.records.get(cls, {}).get('validated'), value)


class BehaviorCalls:
    keys = []

    @classmethod
    def record(cls, key):
        if key not in cls.keys:
            cls.keys.append(key)

    @classmethod
    def take(cls):
        keys = list(cls.keys)
        cls.keys.clear()
        return keys
