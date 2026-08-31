#!/usr/bin/env python3
"""Exercise the real WebKit accessibility tree and native Rust companion."""

import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402


def walk(root):
    """Bound total nodes and depth even when the remote accessibility tree fails."""
    pending = [(root, 0)]
    for _ in range(4096):
        if not pending:
            return
        node, depth = pending.pop()
        if node is None:
            continue
        yield node
        if depth < 24:
            for index in range(min(node.get_child_count(), 128)):
                pending.append((node.get_child_at_index(index), depth + 1))


def application():
    desktop = Atspi.get_desktop(0)
    for index in range(min(desktop.get_child_count(), 128)):
        candidate = desktop.get_child_at_index(index)
        if candidate is not None and candidate.get_name() == "NativeGate":
            return candidate
    return None


def snapshot(root):
    result = set()
    for node in walk(root):
        result.add(node.get_name())
        text = node.get_text_iface()
        if text is not None:
            # Accessible.get_text() shadows the Text interface's range method.
            end = min(Atspi.Text.get_character_count(text), 512)
            result.add(Atspi.Text.get_text(text, 0, end))
    return result


def expect(required, seconds=20):
    deadline = time.monotonic() + seconds
    exposed = set()
    while time.monotonic() < deadline:
        root = application()
        if root is not None:
            exposed = snapshot(root)
            if set(required) <= exposed:
                print("OK:", ", ".join(required), flush=True)
                return root
        time.sleep(0.2)
    raise AssertionError(f"Missing {set(required) - exposed}; exposed: {sorted(exposed)}")


def activate(name):
    root = expect([name])
    for node in walk(root):
        if node.get_name() != name:
            continue
        action = node.get_action_iface()
        if action is not None and action.get_n_actions() and action.do_action(0):
            return
    raise AssertionError(f"Rendered control {name!r} has no accessible activation action")


def main():
    Atspi.init()
    expect(["Root route", "Root count 0", "Shared module ready", "Verify native Root"])
    activate("Verify native Root")
    expect(["Root count 7", "Native Rust 14", "OS ready", "Styles ready",
            "Route boundary ready", "Publish received"])
    activate("Open second")
    expect(["Second route", "Second count 0", "Shared module ready"])
    activate("Verify native Second")
    expect(["Second count 9", "Native Rust 18", "Styles ready",
            "Route boundary ready", "Publish received"])
    activate("Return root")
    expect(["Root route", "Root count 7"])
    activate("Leave app")
    time.sleep(0.5)
    expect(["Root route", "Root count 7", "Verify native Root"])
    print("PASS: rendered WebView/native interaction and navigation boundary", flush=True)


if __name__ == "__main__":
    main()
