#!/usr/bin/env python3
"""Phase 5 Linux accessibility and interaction probe.

Walks the running application's AT-SPI tree, asserts that every accessible
name declared in the source HTML reached the platform accessibility API, then
activates the native button through AT-SPI and asserts that the declarative
state update is visible to assistive technology.
"""

import sys
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402  (must follow require_version)

APPLICATION = "PhaseFive"
REQUIRED_NAMES = [
    "Phase Five demo",  # <main aria-label>
    "Phase Five",  # <h1>
    "Increase count",  # <button aria-label>
    "Count",  # <output aria-label>
    "Your name",  # <input aria-label>
    "More detail",  # <details aria-label>
    "Sales chart",  # WebSurface fallback aria-label
]
MAX_DEPTH = 24
MAX_CHILDREN = 512


def walk(node, depth=0):
    """Yields every accessible in the subtree, bounded by depth.

    An AT-SPI tree is a tree, so depth is the correct bound. Identity-based
    pruning is wrong here: PyGObject hands out fresh wrappers per call, so
    `id()` values are reused and silently discard whole subtrees.
    """
    if depth > MAX_DEPTH:
        return
    yield node
    for index in range(min(node.get_child_count(), MAX_CHILDREN)):
        child = node.get_child_at_index(index)
        if child is not None:
            yield from walk(child, depth + 1)


def find_application(deadline=30.0):
    """Returns the generated application's AT-SPI root once it registers."""
    started = time.monotonic()
    while time.monotonic() - started < deadline:
        desktop = Atspi.get_desktop(0)
        for index in range(desktop.get_child_count()):
            application = desktop.get_child_at_index(index)
            if application is not None and application.get_name() == APPLICATION:
                return application
        time.sleep(0.5)
    return None


def names(root):
    return {node.get_name() for node in walk(root) if node.get_name()}


def await_names(root, required, deadline=30.0):
    """Waits for GTK to publish its lazily-created accessible objects."""
    started = time.monotonic()
    exposed = set()
    while time.monotonic() - started < deadline:
        exposed = names(root)
        if all(name in exposed for name in required):
            return exposed, []
        time.sleep(1.0)
    return exposed, [name for name in required if name not in exposed]


def dump(node, depth=0):
    print(f"{'  ' * depth}{node.get_role_name()!r} {node.get_name()!r}")
    if depth > 12:
        return
    for index in range(node.get_child_count()):
        child = node.get_child_at_index(index)
        if child is not None:
            dump(child, depth + 1)


def main():
    Atspi.init()
    application = find_application()
    if application is None:
        print("FAIL: the generated application never registered with AT-SPI")
        return 1

    exposed, missing = await_names(application, REQUIRED_NAMES)
    if missing:
        print(f"FAIL: accessible names missing from the AT-SPI tree: {missing}")
        print(f"exposed: {sorted(exposed)}")
        print("--- AT-SPI tree ---")
        dump(application)
        return 1
    print(f"OK: {len(REQUIRED_NAMES)} declared accessible names reached AT-SPI")

    roles = {
        node.get_name(): node.get_role_name()
        for node in walk(application)
        if node.get_name() in {"Increase count", "Your name", "More detail"}
    }
    print(f"OK: platform roles {roles}")
    if roles.get("Increase count") not in {"button", "push button"}:
        print("FAIL: the button did not map to a native button")
        return 1
    if roles.get("Your name") not in {"text", "entry"}:
        print("FAIL: the text field did not map to a native entry")
        return 1

    button = next(
        (node for node in walk(application) if node.get_name() == "Increase count"),
        None,
    )
    action = button.get_action_iface() if button is not None else None
    if action is None:
        print("FAIL: the native button exposes no AT-SPI action")
        return 1
    action.do_action(0)
    time.sleep(1.0)

    if "1" not in names(application):
        print("FAIL: increment state never became visible to AT-SPI")
        print(f"exposed: {sorted(names(application))}")
        return 1
    print("OK: AT-SPI activation incremented the bound state to 1")
    return 0


if __name__ == "__main__":
    sys.exit(main())
