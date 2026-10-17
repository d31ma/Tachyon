import asyncio as _tac_asyncio
import inspect as _tac_inspect

class _TacPythonGraph:
    MAX_INSTANCES = 4096
    MAX_DEPTH = 32

    def __init__(self, types, providers, codec, publisher):
        self.types = types
        self.providers = providers
        self.codec = codec
        self.publisher = publisher
        self.shared = {}
        self.starting = None
        self.failed = set()

    def acquire(self, name, owner):
        return self._acquire(name, owner)

    def require_synchronous(self, values):
        self._require_synchronous(values)

    def _acquire(self, name, owner):
        metadata = self.providers[name]
        self._require_graph_bounds(name, owner)
        owner.resolving.add(name)
        try:
            return self._provider(name, metadata, owner)
        finally:
            owner.resolving.remove(name)

    def dispose(self, owner):
        self._dispose(owner)

    def _dispose(self, owner):
        owner.disposed = True
        failures = [self._release(item) for item in reversed(owner.instances)]
        owner.instances.clear()
        owner.retained.clear()
        self._require_stopped(failures)

    def _require_graph_bounds(self, name, owner):
        valid = name not in owner.resolving and len(owner.resolving) < self.MAX_DEPTH
        valid = valid and len(owner.instances) < self.MAX_INSTANCES
        if not valid:
            raise ValueError('Python Tac dependency graph exceeds bounds or contains a cycle.')

    def _provider(self, name, metadata, owner):
        if metadata['prototype']:
            return self._prototype(name, metadata, owner)
        return self._shared(name, metadata, owner)

    def _prototype(self, name, metadata, owner):
        value = self._construct(name, metadata, owner)
        owner.retain_prototype(value, metadata)
        self._start(value, metadata['starts'])
        return value

    def _shared(self, name, metadata, owner):
        if name in self.shared:
            return self._existing(name, metadata, owner)
        return self._new_shared(name, metadata, owner)

    def _existing(self, name, metadata, owner):
        value = self.shared[name][0]
        self._retain_shared(name, value, metadata, owner)
        return value

    def _retain_shared(self, name, value, metadata, owner):
        if name not in owner.retained:
            self.shared[name][1] += 1
            owner.retain(name, value, metadata)

    def _new_shared(self, name, metadata, owner):
        scope = owner.child()
        value = self._shared_value(name, metadata, scope)
        self.shared[name] = [value, 1, scope]
        owner.retain(name, value, metadata)
        self._start(value, metadata['starts'])
        return value

    def _shared_value(self, name, metadata, scope):
        try:
            return self._construct(name, metadata, scope)
        except Exception:
            self.dispose(scope)
            raise

    def _construct(self, name, metadata, owner):
        arguments = [self._argument(binding, owner) for binding in metadata['constructor']]
        value = self.types[name](*arguments)
        for field in metadata['publishers']:
            setattr(value, field, self.publisher(owner))
        return value

    def _argument(self, binding, owner):
        if 'provider' in binding:
            return self.acquire(binding['provider'], owner)
        return self._property(binding, owner)

    def _property(self, binding, owner):
        field = binding['page']
        if field not in owner.properties:
            raise ValueError('Python Tac required property is missing: ' + field)
        return self.codec.read(binding['shape'], owner.properties[field])

    def _release(self, item):
        name, value, metadata = item
        try:
            self._release_owned(name, value, metadata)
            return None
        except Exception as error:
            return error

    def _release_owned(self, name, value, metadata):
        if name is None:
            self._hooks(value, metadata['stops'])
            return
        self._release_shared(name, value, metadata)

    def _release_shared(self, name, value, metadata):
        self.shared[name][1] -= 1
        if self.shared[name][1] == 0:
            scope = self.shared.pop(name)[2]
            self._stop_shared(value, metadata, scope)

    def _stop_shared(self, value, metadata, scope):
        try:
            self._hooks(value, metadata['stops'])
        finally:
            self.dispose(scope)

    def started(self, value):
        return id(value) not in self.failed

    def _start(self, value, names):
        results = [getattr(value, name)() for name in names]
        for started in results:
            if _tac_inspect.isawaitable(started):
                self.starting = _tac_asyncio.ensure_future(self._after(self.starting, value, started))

    async def _after(self, previous, value, started):
        if previous is not None:
            await _tac_asyncio.gather(previous, return_exceptions=True)
        try:
            await started
        except BaseException:
            self.failed.add(id(value))
            raise

    def _hooks(self, value, names):
        results = [getattr(value, name)() for name in names]
        self._require_synchronous(results)

    def _require_synchronous(self, values):
        pending = [value for value in values if _tac_inspect.isawaitable(value)]
        if pending:
            self._close_pending(pending)
            raise ValueError('Python Tac asynchronous lifecycle needs its continuation adapter.')

    def _close_pending(self, values):
        closers = [getattr(value, 'close', None) for value in values]
        [closer() for closer in closers if callable(closer)]

    def _require_stopped(self, failures):
        errors = [error for error in failures if error is not None]
        if errors:
            raise ValueError('Python Tac lifecycle disposal failed.') from errors[0]
