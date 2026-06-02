import os
import sys

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "../../../../../services")))

from python_language_service import PythonLanguageService


class Handler:
    __service = PythonLanguageService()

    @staticmethod
    def GET(request):
        return Handler.__service.version_details(request)

    @staticmethod
    def DELETE(request):
        return Handler.__service.delete_version(request)

    @staticmethod
    def PATCH(request):
        return Handler.__service.patch_version(request)
