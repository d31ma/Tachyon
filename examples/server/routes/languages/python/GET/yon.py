import os
import sys

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "../../../../services")))

from python_language_service import PythonLanguageService

service = PythonLanguageService()

STATUS_RESPONSES = {
    "303": {"code": "303", "location": "/redirect"},
    "304": {},
    "305": {"code": "305", "location": "/redirect"},
    "307": {"code": "307", "location": "/redirect"},
    "308": {"code": "308", "location": "/redirect"},
    "400": {"code": "400", "detail": "bad request"},
}


def handler(request):
    query = request.get("query") or {}
    raw = query.get("code")
    code = str(int(raw)) if isinstance(raw, (int, float)) else str(raw or "")
    if code in STATUS_RESPONSES:
        return STATUS_RESPONSES[code]
    return service.describe(request)
