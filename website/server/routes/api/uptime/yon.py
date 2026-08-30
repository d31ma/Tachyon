import time

# Ported from Perl when Yon narrowed to the languages that can declare a layer.
# The adapter puts @Controller in scope, so nothing is imported for it.
@Controller
class UptimeController:
    STARTED = time.monotonic()

    @staticmethod
    def GET(request):
        return {
            "language": "Python",
            "version": __import__("platform").python_version(),
            "uptimeSeconds": round(time.monotonic() - UptimeController.STARTED, 3),
        }
