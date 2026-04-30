#!/usr/bin/env python3
import asyncio
import importlib.util
import importlib.machinery
import inspect
import json
import os
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
    def route_class_name(handler_path):
        return os.path.basename(handler_path).split(".", 1)[0]

    @staticmethod
    def resolve_handler(module, handler_path):
        handler = getattr(module, "handler", None)
        if callable(handler):
            return handler
        handler_class = getattr(module, YonPythonRunner.route_class_name(handler_path), None)
        if handler_class is not None:
            instance = handler_class()
            method = getattr(instance, "handler", None)
            if callable(method):
                return method
        raise RuntimeError("Python route must define handler(request) or a method-named class with handler(request)")

    @staticmethod
    async def call(handler, request):
        result = handler(request)
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
        module = YonPythonRunner.load_module(sys.argv[1])
        handler = YonPythonRunner.resolve_handler(module, sys.argv[1])
        YonPythonRunner.write(await YonPythonRunner.call(handler, request))


if __name__ == "__main__":
    try:
        asyncio.run(YonPythonRunner.run())
    except Exception as error:
        sys.stderr.write(str(error))
        sys.exit(1)
