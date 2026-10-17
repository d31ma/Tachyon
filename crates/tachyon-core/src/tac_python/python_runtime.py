import asyncio as _tac_asyncio
import builtins as _tac_builtins
import json as _tac_json
import inspect as _tac_inspect
import uuid as _tac_uuid

class _TacPythonTask:
    CANCELLED = {'error': 'The service was removed before its task finished.', 'code': 'CANCELLED'}
    FAILED = {'error': 'Python Tac operation failed.'}

    def __init__(self, number):
        self.number = number

class _TacPythonRuntime:
    MAX_OWNERS = 256
    MAX_REQUEST = 1048576
    MAX_MEMBER_REQUEST = 65536
    MAX_RETURNED_EVENTS = 32
    instance = None

    @classmethod
    def configure(cls, types, metadata, models):
        cls.instance = cls(types, metadata, models)

    @classmethod
    def connect(cls, emit, done=None, storage=None, http=None):
        cls.instance.emit = emit
        cls.instance.done = done
        if storage is not None:
            _tac_builtins.TacStorage = storage
        if http is not None:
            _tac_builtins.TacHttp = http

    @classmethod
    def invoke(cls, text):
        return cls.instance.exchange(text)

    @classmethod
    def shutdown(cls):
        return cls.instance.release_all()

    def release_all(self):
        return self._release_all()

    def _release_all(self):
        owners = list(self.owners.values())
        self.owners.clear()
        failures = [self._release_owner(owner) for owner in owners]
        return all(error is None for error in failures)

    def _release_owner(self, owner):
        try:
            self.graph.dispose(owner)
            return None
        except Exception as error:
            return error

    def __init__(self, types, metadata, models):
        self.metadata = metadata
        self.codec = _TacPythonCodec(models, metadata['models'])
        self.graph = _TacPythonGraph(types, metadata['providers'], self.codec, self._publisher)
        self.owners = {}
        self.busy = False
        self.emit = None
        self.done = None
        self.last_task = 0

    def exchange(self, text):
        return self._exchange(text)

    def _exchange(self, text):
        self._require_idle()
        self.busy = True
        try:
            return self._answer(text)
        finally:
            self.busy = False

    def _answer(self, text):
        try:
            request = self._request(text)
            return self._response(self._dispatch(request))
        except Exception:
            return _tac_json.dumps({'error': 'Python Tac operation failed.'})

    def _require_idle(self):
        if self.busy:
            raise ValueError('Python Tac owner is busy.')

    def _request(self, text):
        if type(text) is not str or len(text.encode('utf-8')) > self.MAX_REQUEST:
            raise ValueError('Python Tac request exceeds bounds.')
        request = _tac_json.loads(text)
        self._request_identity(request)
        self._member_size(request, text)
        return request

    def _request_identity(self, request):
        if type(request) is not dict or request.get('route') != 'web':
            raise ValueError('Python Tac requires its owned web request.')
        self._owner_id(request.get('owner'))
        self._service_name(request.get('service'))

    def _owner_id(self, owner):
        valid = type(owner) is str and len(owner) == 36 and str(_tac_uuid.UUID(owner)) == owner
        if not valid:
            raise ValueError('Python Tac requires a valid owner UUID.')

    def _service_name(self, service):
        if service not in self.metadata['services']:
            raise ValueError('Python Tac service was not registered.')

    def _member_size(self, request, text):
        if request.get('op') != 'init' and len(text.encode('utf-8')) > self.MAX_MEMBER_REQUEST:
            raise ValueError('Python Tac member request exceeds bounds.')

    def _dispatch(self, request):
        handlers = {'init': self._initialize, 'dispose': self._dispose,
                    'get': self._get, 'set': self._set, 'call': self._call,
                    'event': self._event, 'takeEvents': self._take_events}
        return handlers[request['op']](request)

    def _initialize(self, request):
        if request['owner'] in self.owners:
            return self._describe(self._owner(request))
        return self._new_owner(request)

    def _new_owner(self, request):
        self._owner_capacity()
        owner = _TacPythonOwner(request)
        self._properties(owner.properties)
        try:
            name = self.metadata['services'][owner.service]
            owner.root = self.graph.acquire(name, owner)
            owner.starting = self.graph.starting
            self.owners[owner.id] = owner
            return self._describe(owner)
        except Exception:
            self.graph.dispose(owner)
            raise

    def _owner_capacity(self):
        if len(self.owners) >= self.MAX_OWNERS:
            raise ValueError('Python Tac owner capacity was exceeded.')

    def _properties(self, properties):
        if type(properties) is not dict:
            raise ValueError('Python Tac properties must be a JSON object.')
        self.codec.plain(properties)

    def _describe(self, owner):
        metadata = self._metadata(owner)
        listeners = [self._listener_description(listener) for listener in metadata['listeners'].values()]
        return {'fields': list(metadata['fields']), 'writable': metadata['writable'],
                'methods': list(metadata['methods']), 'listeners': listeners}

    def _listener_description(self, listener):
        names = ['key', 'events', 'replay', 'order', 'asynchronous', 'condition', 'models']
        return {name: listener[name] for name in names}

    def _metadata(self, owner):
        name = self.metadata['services'][owner.service]
        return self.metadata['providers'][name]

    def _owner(self, request):
        owner = self.owners[request['owner']]
        owner.require_identity(request)
        return owner

    def _dispose(self, request):
        if request['owner'] in self.owners:
            owner = self._owner(request)
            del self.owners[owner.id]
            self._cancel_tasks(owner)
            self.graph.dispose(owner)
        return None

    def _cancel_tasks(self, owner):
        for running in list(owner.tasks.values()):
            running.cancel()

    def _get(self, request):
        owner = self._owner(request)
        field = self._metadata(owner)['fields'][request['name']]
        return self.codec.result(field['shape'], getattr(owner.root, field['name']))

    def _set(self, request):
        owner = self._owner(request)
        metadata = self._metadata(owner)
        self._writable(metadata, request['name'])
        field = metadata['fields'][request['name']]
        value = self.codec.read(field['shape'], request.get('value'))
        setattr(owner.root, field['name'], value)
        return None

    def _writable(self, metadata, name):
        if name not in metadata['writable']:
            raise ValueError('Python Tac field is not writable.')

    def _call(self, request):
        owner = self._owner(request)
        method = self._metadata(owner)['methods'][request['name']]
        arguments = self._arguments(method, request.get('args', []))
        if not self._started(owner):
            return self._task(owner, self._after_start(owner, method, arguments))
        value = getattr(owner.root, method['name'])(*arguments)
        if _tac_inspect.isawaitable(value):
            return self._task(owner, self._settled(method, value))
        return self.codec.result(method['returns'], value)

    def _started(self, owner):
        return owner.starting is None or owner.starting.done()

    async def _after_start(self, owner, method, arguments):
        await _tac_asyncio.gather(owner.starting, return_exceptions=True)
        if not self.graph.started(owner.root):
            raise ValueError('Python Tac service did not start.')
        value = getattr(owner.root, method['name'])(*arguments)
        return await self._settled(method, value)

    async def _settled(self, method, value):
        if _tac_inspect.isawaitable(value):
            value = await value
        return self.codec.result(method['returns'], value)

    def _task(self, owner, work):
        self.last_task += 1
        number = self.last_task
        running = _tac_asyncio.ensure_future(work)
        owner.tasks[number] = running
        running.add_done_callback(lambda finished: self._finish(owner, number, finished))
        return _TacPythonTask(number)

    def _finish(self, owner, number, finished):
        owner.tasks.pop(number, None)
        self.done(number, self._task_answer(finished))

    def _task_answer(self, finished):
        if finished.cancelled():
            return _tac_json.dumps(_TacPythonTask.CANCELLED)
        error = finished.exception()
        if error is not None:
            return _tac_json.dumps(self._failure(error), allow_nan=False)
        try:
            return self._response(finished.result())
        except Exception:
            return _tac_json.dumps(_TacPythonTask.FAILED)

    def _failure(self, error):
        if type(error).__name__ != 'BehaviorError':
            return _TacPythonTask.FAILED
        message = str(error)[:1024]
        return {'error': message, 'code': str(getattr(error, 'code', 'behavior'))[:64]}

    def _arguments(self, method, values):
        if type(values) is not list or len(values) != len(method['arguments']):
            raise ValueError('Python Tac method argument count does not match.')
        return [self.codec.read(shape, value) for shape, value in zip(method['arguments'], values)]

    def _synchronous(self, value):
        if _tac_inspect.isawaitable(value):
            self.graph.require_synchronous([value])
            raise ValueError('Python Tac async methods need their continuation adapter.')

    def _event(self, request):
        owner = self._owner(request)
        listener = self._metadata(owner)['listeners'][request['name']]
        arguments = self._event_arguments(listener, request.get('value'))
        result = getattr(owner.root, listener['key'])(*arguments)
        self._synchronous(result)
        if not listener['asynchronous']:
            self._republish(owner, listener['returns'], result)
        return None

    def _event_arguments(self, listener, value):
        if listener['argument'] is None:
            return []
        return [self.codec.read(listener['argument'], value)]

    def _republish(self, owner, returns, result):
        if returns is None or result is None:
            return
        items = result if type(result) is list else [result]
        for item in items[:self.MAX_RETURNED_EVENTS]:
            self.publish(owner, returns, item, False)

    def _publisher(self, owner):
        return _TacPythonPublisher(self, owner)

    def publish(self, owner, name, value, retain):
        if owner.disposed or self.emit is None:
            raise ValueError('Python Tac events need a live owner on the page.')
        signal = {'route': owner.route, 'service': owner.service, 'owner': owner.id,
                  'name': name, 'value': self.codec.result({'model': name}, value), 'retain': retain}
        self.emit(_tac_json.dumps(signal, allow_nan=False))

    def _take_events(self, request):
        self._owner(request)
        return []

    def _response(self, value):
        if isinstance(value, _TacPythonTask):
            return _tac_json.dumps({'task': value.number})
        answer = _tac_json.dumps({'value': self.codec.plain(value)}, allow_nan=False)
        if len(answer.encode('utf-8')) > self.MAX_REQUEST:
            raise ValueError('Python Tac response exceeds bounds.')
        return answer
