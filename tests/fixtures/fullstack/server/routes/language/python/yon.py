import os
import sys

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "../../../services")))

from python_language_service import PythonLanguageService


class Handler:
    __service = PythonLanguageService()
    __STATUS_RESPONSES = {
        "303": {"code": "303", "location": "/redirect"},
        "304": {},
        "305": {"code": "305", "location": "/redirect"},
        "307": {"code": "307", "location": "/redirect"},
        "308": {"code": "308", "location": "/redirect"},
        "400": {"code": "400", "detail": "bad request"},
    }

    @staticmethod
    def GET(request):
        code = Handler.__status_code(request)
        if code in Handler.__STATUS_RESPONSES:
            return Handler.__STATUS_RESPONSES[code]
        return Handler.__service.describe(request)

    @staticmethod
    def __status_code(request):
        query = request.get("query") or {}
        raw = query.get("code")
        return str(int(raw)) if isinstance(raw, (int, float)) else str(raw or "")
