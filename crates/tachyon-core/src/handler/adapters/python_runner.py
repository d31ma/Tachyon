import asyncio
import contextlib
import importlib.util
import inspect
import json
import os
import queue
import struct
import subprocess
import sys
import threading
import time

MAX_FRAME_BYTES = 16 * 1024 * 1024
MAX_RELAY_STDOUT_BYTES = 16 * 1024 * 1024
MAX_RELAY_STDERR_BYTES = 64 * 1024
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


# The class that carried `@Controller`, in a list because the decorator runs
# inside an import and cannot rebind a module global from there.
_CONTROLLER = [None]


def _remember(cls):
    _CONTROLLER[0] = cls
    return cls


def _relay_decorator(*command):
    def proxy(_method):
        def relayed(request):
            return _relay(command, request)

        return relayed

    return proxy


def _relay(command, request):
    """Runs a handler written in a language Yon does not run.

    The command is explicit rather than inferred from the file name: a compiled
    language has no interpreter to infer. The working directory is the project
    root, so a project-relative path reads the way it is written.
    """
    if not command:
        return _relay_failed(request, "start")
    try:
        child = subprocess.Popen(
            list(command),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except OSError:
        return _relay_failed(request, "start")

    def drain(stream, limit, result):
        captured = bytearray()
        oversized = False
        try:
            while True:
                part = stream.read(8192)
                if not part:
                    break
                remaining = limit - len(captured)
                if remaining > 0:
                    captured.extend(part[:remaining])
                oversized = oversized or len(part) > remaining
        finally:
            stream.close()
            result.extend((bytes(captured), oversized))

    stdout_result = []
    stderr_result = []
    stdout_thread = threading.Thread(
        target=drain,
        args=(child.stdout, MAX_RELAY_STDOUT_BYTES, stdout_result),
        daemon=True,
    )
    stderr_thread = threading.Thread(
        target=drain,
        args=(child.stderr, MAX_RELAY_STDERR_BYTES, stderr_result),
        daemon=True,
    )
    stdout_thread.start()
    stderr_thread.start()
    try:
        child.stdin.write(json.dumps(request).encode("utf-8"))
    except (BrokenPipeError, OSError):
        pass
    finally:
        child.stdin.close()

    deadline = time.monotonic() + max(
        0.001, float(request.get("deadline_ms", 30_000)) / 1000.0
    )
    while child.poll() is None and time.monotonic() < deadline:
        time.sleep(0.002)
    timed_out = child.poll() is None
    if timed_out:
        child.kill()
    remaining = max(0.0, deadline - time.monotonic())
    stdout_thread.join(remaining)
    stderr_thread.join(max(0.0, deadline - time.monotonic()))
    if stdout_thread.is_alive() or stderr_thread.is_alive():
        child.kill()
        return _relay_failed(request, "timeout")
    child.wait()
    stdout, stdout_oversized = stdout_result
    _, stderr_oversized = stderr_result
    if timed_out:
        return _relay_failed(request, "timeout")
    if stdout_oversized or stderr_oversized:
        return _relay_failed(request, "overflow")
    if child.returncode != 0:
        return _relay_failed(request, "exit")
    try:
        envelope = json.loads(stdout.decode("utf-8"))
    except (UnicodeDecodeError, ValueError):
        return _relay_failed(request, "protocol")
    # Returned in the shape a handler may return directly, so the runner's own
    # descriptor check writes it — the delegate's status and headers travel
    # without this shim re-implementing the envelope.
    return {
        "status": envelope.get("status", 200),
        "headers": envelope.get("headers", {}),
        "body": envelope.get("body", ""),
    }


def _relay_failed(request, category, reason="Delegate invocation failed."):
    """Return a stable public error; process details stay on the sideband."""
    sys.stderr.write(json.dumps({
        "event": "handler.relay_failed",
        "request_id": request.get("request_id", "unknown"),
        "category": category,
    }, separators=(",", ":")) + "\n")
    return {
        "status": 502,
        "headers": {"content-type": ["application/json"]},
        "body": json.dumps({"error": reason}),
    }


def load_handler():
    sys.path.insert(0, os.path.dirname(source))
    sys.path.insert(1, project_root)
    specification = importlib.util.spec_from_file_location(
        "tachyon_yon_handler", source
    )
    if specification is None or specification.loader is None:
        raise RuntimeError("Cannot load the Python handler module.")
    # The layer stereotypes, in builtins so no handler has to import them.
    #
    # A decorator has to return the class unchanged: the annotation states
    # which layer a class is in and must not alter it. Set only when absent, so
    # a project that defines its own keeps it.
    import builtins

    # Capitalised, against PEP 8's rule for functions, because a stereotype is
    # a name shared across eight languages rather than an ordinary function —
    # one spelling everywhere beats a table of per-language casing.
    # Work handed to a language Yon does not run.
    #
    # Yon runs the eight languages that can declare a layer. Go, Ruby, Elixir
    # and the rest cannot, so they are not routes — but they are still
    # programs, and a program that speaks Handler Protocol v1 on standard input
    # and output is exactly what Yon spawns anyway.
    #
    # In builtins for the same reason the stereotypes are: an import line to
    # reach something the runtime already supplies is a tax that gets it
    # dropped.
    if not hasattr(builtins, "relay"):
        builtins.relay = _relay
    # `@Relay("ruby", "server/delegates/report.rb")` on a method makes that
    # method a proxy. The command is metadata about the method, so it belongs
    # in the declaration rather than in a body the reader has to open.
    #
    # Python is one of the four languages where the decorator can do the work
    # itself: it returns the function that replaces the one it was written on,
    # so nothing intercepts the call later.
    if not hasattr(builtins, "Relay"):
        builtins.Relay = _relay_decorator
    # `@Stream` marks a method that answers more than once. It does nothing at
    # run time — `yield` in the body is what streams — and exists so the server
    # can decide which path a route takes before it calls the handler, and so
    # the two can be checked against each other before it is built.
    if not hasattr(builtins, "Stream"):
        builtins.Stream = lambda method: method
    for layer in ("Controller", "Service", "Repository", "Client", "Delegate"):
        if not hasattr(builtins, layer):
            # `@Controller` is how the handler class says it is the handler
            # class, so the stub remembers what it was put on. That is cheaper
            # than looking for a class named `Handler`, and it is what lets the
            # class be called `OrdersController` — which the suffix rule wants.
            setattr(builtins, layer, _remember if layer == "Controller" else (lambda cls: cls))

    module = importlib.util.module_from_spec(specification)
    with contextlib.redirect_stdout(sys.stderr):
        specification.loader.exec_module(module)
    handler = _CONTROLLER[0]
    if not inspect.isclass(handler):
        raise TypeError("Module must define a class carrying @Controller.")
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


def write_event(request_id, value):
    """Writes one streamed event frame without settling the request."""
    payload = json.dumps(
        {
            "protocol_version": 1,
            "kind": "event",
            "request_id": request_id,
            # The same separators the response path uses. Without them a
            # streamed event arrives spaced where the JavaScript adapter's is
            # compact, so the same handler in two languages sent two different
            # payloads for the same value.
            "body": {
                "encoding": "utf8",
                "data": json.dumps(value, ensure_ascii=False, separators=(",", ":")),
            },
        }
    ).encode("utf-8")
    if len(payload) > MAX_FRAME_BYTES:
        write_frame(
            failure(
                request_id,
                "TY2203",
                "Streamed event exceeds the protocol frame limit.",
            )
        )
        return False
    protocol_output.write(struct.pack(">I", len(payload)) + payload)
    protocol_output.flush()
    return True


def stream_events(request_id, iterator):
    """Drains a generator into event frames, then ends by closing the stream.

    `yield` is how a handler says it has more than one thing to send, in the
    language's own words rather than a framework call. End of stream is end of
    process, so there is no terminator frame to keep in step with.
    """
    for event in iterator:
        if not write_event(request_id, event):
            os._exit(0)
    protocol_output.flush()
    os._exit(0)


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
        if inspect.isgenerator(result):
            stream_events(request["request_id"], result)
            return
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
