import json
import os
import subprocess


class PythonFyloRepository:
    def __init__(self, root=None):
        self.root = root or os.environ.get(
            "FYLO_ROOT",
            os.path.abspath(os.path.join(os.getcwd(), "db")),
        )
        self.executable = (
            os.environ.get("FYLO_EXEC_PATH")
            or os.environ.get("FYLO_BINARY")
            or "fylo"
        )

    def machine(self, request):
        command = [self.executable, "exec", "--request", "-", "--root", self.root]
        process = subprocess.run(
            command,
            input=json.dumps(request),
            text=True,
            capture_output=True,
            check=False,
        )
        if process.returncode != 0:
            raise RuntimeError(process.stderr or process.stdout or "fylo exec failed")
        response = json.loads(process.stdout or "{}")
        if not response.get("ok"):
            error = response.get("error", {})
            raise RuntimeError(error.get("message", "fylo exec returned an error"))
        return response.get("result")

    def write_sample(self, language, request_id):
        collection = "language-route-events"
        self.machine({"op": "createCollection", "collection": collection})
        document = {
            "language": language,
            "source": "fylo exec",
            "requestId": request_id,
        }
        doc_id = self.machine({"op": "putData", "collection": collection, "data": document})
        found = self.machine({
            "op": "findDocs",
            "collection": collection,
            "query": {"$ops": [{"language": {"$eq": language}}]},
        })
        return {
            "collection": collection,
            "id": doc_id,
            "document": document,
            "matched": str(len(found.keys()) if isinstance(found, dict) else 0),
            "operations": ["createCollection", "putData", "findDocs"],
            "resultCount": "3",
        }
