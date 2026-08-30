"""Exercise the ty server's observable Windows CTRL-BREAK shutdown."""

from __future__ import annotations

import json
import os
import pathlib
import queue
import signal
import subprocess
import sys
import tempfile
import threading
from typing import TextIO


def lines(pipe: TextIO) -> queue.Queue[str]:
    output: queue.Queue[str] = queue.Queue()

    def read() -> None:
        for line in pipe:
            output.put(line.rstrip("\r\n"))

    threading.Thread(target=read, daemon=True).start()
    return output


def next_line(output: queue.Queue[str], description: str) -> str:
    try:
        return output.get(timeout=10)
    except queue.Empty as error:
        raise RuntimeError(f"timed out waiting for {description}") from error


def main() -> None:
    if len(sys.argv) != 2:
        raise RuntimeError("usage: signal-lifecycle.py <ty.exe>")
    binary = pathlib.Path(sys.argv[1]).resolve()
    with tempfile.TemporaryDirectory(prefix="tachyon-signal-") as temporary:
        project = pathlib.Path(temporary)
        page = project / "client" / "pages" / "tac.html"
        page.parent.mkdir(parents=True)
        page.write_text('<main aria-label="Signal"><h1>Signal</h1></main>', encoding="utf-8")
        child = subprocess.Popen(
            [
                str(binary),
                "serve",
                str(project),
                "--host",
                "127.0.0.1",
                "--port",
                "0",
                "--no-watch",
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
        )
        try:
            if child.stdout is None or child.stderr is None:
                raise RuntimeError("server pipes were not created")
            stdout = lines(child.stdout)
            stderr = lines(child.stderr)
            startup = json.loads(next_line(stderr, "handler startup"))
            if startup.get("installed") != ["CTRL_C", "CTRL_BREAK"]:
                raise RuntimeError(f"unexpected startup event: {startup}")
            ready = next_line(stdout, "server readiness")
            if "Tachyon server ready" not in ready:
                raise RuntimeError(f"unexpected readiness line: {ready}")

            os.kill(child.pid, signal.CTRL_BREAK_EVENT)
            receipt = json.loads(next_line(stderr, "CTRL-BREAK receipt"))
            if receipt.get("signal") != "CTRL_BREAK":
                raise RuntimeError(f"unexpected receipt event: {receipt}")
            status = child.wait(timeout=10)
            if status != 0:
                raise RuntimeError(f"server exited with status {status}")
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=5)


if __name__ == "__main__":
    main()
