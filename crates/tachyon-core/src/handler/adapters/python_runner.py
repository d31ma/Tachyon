import asyncio
import contextlib
import importlib.util
import inspect
import json
import os
import queue
import struct
import sys
import threading

MAX_FRAME_BYTES = 16 * 1024 * 1024
protocol_output = sys.stdout.buffer
source = sys.argv[1]
project_root = sys.argv[2]
incoming = queue.Queue()
completed = queue.Queue()


def public_message(error):
    value = str(error) or "Handler failed."
    return value[:2048]


def write_frame(envelope):
    try:
        payload = json.dumps(
            envelope, ensure_ascii=False, separators=(",", ":")
        ).encode("utf-8")
    except Exception:
        payload = json.dumps(
            {
                "protocol_version": 1,
                "kind": "response",
                "request_id": envelope.get("request_id", "adapter_error"),
                "status": 500,
                "headers": {},
                "error": {
                    "code": "TY2203",
                    "message": "Handler returned a value that is not JSON-serializable.",
                    "retryable": False,
                },
            },
            separators=(",", ":"),
        ).encode("utf-8")
    if len(payload) > MAX_FRAME_BYTES:
        payload = json.dumps(
            {
                "protocol_version": 1,
                "kind": "response",
                "request_id": envelope.get("request_id", "adapter_error"),
                "status": 500,
                "headers": {},
                "error": {
                    "code": "TY2203",
                    "message": "Serialized handler result exceeds the protocol frame limit.",
                    "retryable": False,
                },
            },
            separators=(",", ":"),
        ).encode("utf-8")
    protocol_output.write(struct.pack(">I", len(payload)))
    protocol_output.write(payload)
    protocol_output.flush()


def failure(request_id, code, message, status=500):
    return {
        "protocol_version": 1,
        "kind": "response",
        "request_id": request_id,
        "status": status,
        "headers": {},
        "error": {
            "code": code,
            "message": str(message)[:2048] or "Handler failed.",
            "retryable": False,
        },
    }


def read_exact(stream, length):
    parts = bytearray()
    while len(parts) < length:
        part = stream.read(length - len(parts))
        if not part:
            return None
        parts.extend(part)
    return bytes(parts)


def read_frames():
    while True:
        prefix = read_exact(sys.stdin.buffer, 4)
        if prefix is None:
            return
        length = struct.unpack(">I", prefix)[0]
        if length > MAX_FRAME_BYTES:
            os._exit(70)
        payload = read_exact(sys.stdin.buffer, length)
        if payload is None:
            os._exit(70)
        try:
            incoming.put(json.loads(payload.decode("utf-8")))
        except Exception:
            os._exit(70)


def load_handler():
    sys.path.insert(0, os.path.dirname(source))
    sys.path.insert(1, project_root)
    specification = importlib.util.spec_from_file_location(
        "tachyon_yon_handler", source
    )
    if specification is None or specification.loader is None:
        raise RuntimeError("Cannot load the Python handler module.")
    module = importlib.util.module_from_spec(specification)
    with contextlib.redirect_stdout(sys.stderr):
        specification.loader.exec_module(module)
    handler = getattr(module, "Handler", None)
    if not inspect.isclass(handler):
        raise TypeError("Module must define a Handler class.")
    return handler


def response_descriptor(value):
    if not isinstance(value, dict) or "headers" not in value:
        return None
    if not set(value).issubset({"status", "headers", "body"}):
        return None
    status = value.get("status", 200)
    if isinstance(status, bool) or not isinstance(status, int) or not 100 <= status <= 599:
        raise TypeError("Handler response status must be an integer from 100 through 599.")
    raw_headers = value["headers"]
    if not isinstance(raw_headers, dict):
        raise TypeError("Handler response headers must be an object.")
    headers = {}
    for name, raw in raw_headers.items():
        if not isinstance(name, str):
            raise TypeError("Handler response header names must be strings.")
        values = raw if isinstance(raw, list) else [raw]
        if not values or any(not isinstance(item, str) for item in values):
            raise TypeError(f"Handler response header '{name}' must contain strings.")
        headers[name.lower()] = values
    body = None
    if "body" in value:
        data = (
            value["body"]
            if isinstance(value["body"], str)
            else json.dumps(value["body"], ensure_ascii=False, separators=(",", ":"))
        )
        body = {"encoding": "utf8", "data": data}
    return {"status": status, "headers": headers, "body": body}


def invoke(handler, request):
    try:
        descriptor = inspect.getattr_static(handler, request["method"], None)
        if not isinstance(descriptor, staticmethod):
            completed.put(
                failure(
                    request["request_id"],
                    "TY2202",
                    f"Handler does not define static {request['method']}().",
                    405,
                )
            )
            return
        method = descriptor.__func__
        with contextlib.redirect_stdout(sys.stderr):
            result = method(request)
            if inspect.isawaitable(result):
                result = asyncio.run(result)
        explicit = response_descriptor(result)
        if explicit is not None:
            response = {
                "protocol_version": 1,
                "kind": "response",
                "request_id": request["request_id"],
                "status": explicit["status"],
                "headers": explicit["headers"],
            }
            if explicit["body"] is not None:
                response["body"] = explicit["body"]
            completed.put(response)
            return
        data = json.dumps(result, ensure_ascii=False, separators=(",", ":"))
        completed.put(
            {
                "protocol_version": 1,
                "kind": "response",
                "request_id": request["request_id"],
                "status": 200,
                "headers": {
                    "content-type": ["application/json; charset=utf-8"]
                },
                "body": {"encoding": "utf8", "data": data},
            }
        )
    except (TypeError, ValueError) as error:
        completed.put(failure(request["request_id"], "TY2203", public_message(error)))
    except BaseException as error:
        completed.put(failure(request["request_id"], "TY2201", public_message(error)))


def main():
    threading.Thread(target=read_frames, daemon=True).start()
    request = incoming.get()
    if (
        not isinstance(request, dict)
        or request.get("protocol_version") != 1
        or request.get("kind") != "request"
        or not isinstance(request.get("request_id"), str)
    ):
        os._exit(70)
    try:
        handler = load_handler()
    except BaseException as error:
        write_frame(failure(request["request_id"], "TY2201", public_message(error)))
        os._exit(0)
    threading.Thread(target=invoke, args=(handler, request), daemon=True).start()
    while True:
        try:
            response = completed.get(timeout=0.02)
            write_frame(response)
            os._exit(0)
        except queue.Empty:
            pass
        try:
            envelope = incoming.get_nowait()
        except queue.Empty:
            continue
        if (
            isinstance(envelope, dict)
            and envelope.get("kind") == "cancel"
            and envelope.get("request_id") == request["request_id"]
        ):
            write_frame(
                failure(
                    request["request_id"],
                    "TY2111",
                    "Handler invocation was cancelled.",
                    499,
                )
            )
            os._exit(0)


main()
