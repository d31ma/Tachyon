class _TacPythonOwner:
    def __init__(self, request):
        self.id = request['owner']
        self.service = request['service']
        self.route = request['route']
        self.properties = request.get('props', {})
        self.retained = set()
        self.instances = []
        self.resolving = set()
        self.root = None
        self.disposed = False
        self.starting = None
        self.tasks = {}

    def require_identity(self, request):
        if self.service != request['service'] or self.route != request['route']:
            raise ValueError('Python Tac owner does not match this service.')

    def retain(self, name, instance, metadata):
        self.retained.add(name)
        self.instances.append((name, instance, metadata))

    def retain_prototype(self, instance, metadata):
        self.instances.append((None, instance, metadata))

    def child(self):
        scope = type(self)({'owner': self.id, 'service': self.service,
                            'route': self.route, 'props': self.properties})
        scope.resolving = self.resolving.copy()
        return scope
