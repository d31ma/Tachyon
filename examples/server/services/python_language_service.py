import os
import sys

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "../repositories")))

from python_language_repository import PythonLanguageRepository


class PythonLanguageService:
    def __init__(self, repository=None):
        self.repository = repository or PythonLanguageRepository()

    def describe(self, request):
        return {
            "language": "python",
            "message": "Hello from Python!",
            "requestId": self.repository.request_id(request),
        }

    def version_details(self, request):
        return {
            "message": "Hello from Yon version service!",
            "version": self.repository.version(request),
            "context": request.get("context", {}),
        }

    def delete_version(self, request):
        return {
            "message": "Hello from Yon version service!",
            "method": "DELETE",
            "version": self.repository.version(request),
            "context": request.get("context", {}),
        }

    def patch_version(self, request):
        paths = request.get("paths", {})
        return {
            "message": "Hello from Yon version service!",
            "method": "PATCH",
            "version": self.repository.version(request),
            "path": paths,
            "body": request.get("body"),
            "context": request.get("context", {}),
        }
