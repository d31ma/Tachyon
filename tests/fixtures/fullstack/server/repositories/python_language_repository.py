class PythonLanguageRepository:
    def request_id(self, request):
        context = request.get("context", {}) if isinstance(request, dict) else {}
        return context.get("requestId", "unknown")

    def version(self, request):
        paths = request.get("paths", {}) if isinstance(request, dict) else {}
        return str(paths.get("version", "v1"))
