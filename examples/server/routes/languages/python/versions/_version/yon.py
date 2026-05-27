import os
import sys

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "../../../../../services")))

from python_language_service import PythonLanguageService

service = PythonLanguageService()


class Handler:
    @staticmethod
    def GET(request):
        return service.version_details(request)

    @staticmethod
    def DELETE(request):
        return service.delete_version(request)

    @staticmethod
    def PATCH(request):
        return service.patch_version(request)
