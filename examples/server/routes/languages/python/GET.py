import os
import sys

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "../../../services")))

from python_language_service import PythonLanguageService

service = PythonLanguageService()


def handler(request):
    return service.describe(request)
