#!/usr/bin/env python3
import asyncio
import importlib.util
import importlib.machinery
import inspect
import json
import sys


class YonPythonRunner:
    @staticmethod
    def load_module(handler_path):
        loader = importlib.machinery.SourceFileLoader("yon_user_handler", handler_path)
        spec = importlib.util.spec_from_loader(loader.name, loader)
        if spec is None:
            raise RuntimeError(f"Unable to load handler module: {handler_path}")
        module = importlib.util.module_from_spec(spec)
        loader.exec_module(module)
        return module

    @staticmethod
    def resolve_handler_class(module):
        handler_class = getattr(module, "Handler", None)
        if handler_class is None or not isinstance(handler_class, type):
            raise RuntimeError("Python route must define a class named Handler")
        return handler_class

    @staticmethod
    def resolve_method(handler_class, method):
        fn = getattr(handler_class, method, None)
        if fn is None or not callable(fn):
            raise RuntimeError(f"Handler class does not implement {method}()")
        return fn

    @staticmethod
    async def call(fn, request):
        result = fn(request)
        if inspect.isawaitable(result):
            return await result
        return result

    @staticmethod
    def write(value):
        if value is None:
            return
        if isinstance(value, str):
            sys.stdout.write(value)
            return
        sys.stdout.write(json.dumps(value, separators=(",", ":")))

    @staticmethod
    async def run():
        if len(sys.argv) < 2:
            raise RuntimeError("Missing handler path")
        request = json.loads(sys.stdin.read() or "{}")
        method = request.get("method")
        if not method:
            raise RuntimeError("Missing HTTP method in request payload")
        module = YonPythonRunner.load_module(sys.argv[1])
        handler_class = YonPythonRunner.resolve_handler_class(module)
        dispatch = YonPythonRunner.resolve_method(handler_class, method)
        YonPythonRunner.write(await YonPythonRunner.call(dispatch, request))


if __name__ == "__main__":
    try:
        asyncio.run(YonPythonRunner.run())
    except Exception as error:
        sys.stderr.write(str(error))
        sys.exit(1)
