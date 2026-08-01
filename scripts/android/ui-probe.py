#!/usr/bin/env python3
"""Phase 5 Android accessibility and interaction probe.

Reads a `uiautomator dump` and asserts, depending on the requested mode, that
declared accessible names reached native widgets, that the button and text
field are the platform's own controls, or that a bound state value is visible.

Usage:
  ui-probe.py <ui.xml> names
  ui-probe.py <ui.xml> locate "<content-desc>"
  ui-probe.py <ui.xml> expect-text "<value>"
"""

import re
import sys
import xml.etree.ElementTree as ET

REQUIRED = [
    "Phase Five demo",  # <main aria-label>
    "Increase count",  # <button aria-label>
    "Count",  # <output aria-label>
    "Your name",  # <input aria-label>
    "More detail",  # <details aria-label>
]
NATIVE_CLASSES = {"Increase count": "Button", "Your name": "EditText"}


def nodes(path):
    with open(path, encoding="utf-8") as handle:
        return list(ET.fromstring(handle.read()).iter("node"))


def check_names(tree):
    described = {n.get("content-desc") for n in tree if n.get("content-desc")}
    missing = [name for name in REQUIRED if name not in described]
    if missing:
        print(f"FAIL: content descriptions missing: {missing}")
        print(f"exposed: {sorted(described)}")
        return 1
    classes = {
        n.get("content-desc"): n.get("class", "").split(".")[-1]
        for n in tree
        if n.get("content-desc") in NATIVE_CLASSES
    }
    for name, expected in NATIVE_CLASSES.items():
        if classes.get(name) != expected:
            print(f"FAIL: {name!r} is {classes.get(name)!r}, expected {expected!r}")
            return 1
    print(f"OK: {len(REQUIRED)} accessible names on native widgets {classes}")
    return 0


def locate(tree, description):
    for node in tree:
        if node.get("content-desc") == description:
            x1, y1, x2, y2 = map(int, re.findall(r"\d+", node.get("bounds")))
            print((x1 + x2) // 2, (y1 + y2) // 2)
            return 0
    print(f"FAIL: no node described as {description!r}", file=sys.stderr)
    return 1


def expect_text(tree, value):
    texts = {n.get("text") for n in tree if n.get("text")}
    if value not in texts:
        print(f"FAIL: {value!r} is not visible; saw {sorted(texts)}")
        return 1
    print(f"OK: the bound output reads {value!r}")
    return 0


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 2
    tree = nodes(argv[1])
    mode = argv[2]
    if mode == "names":
        return check_names(tree)
    if mode == "locate":
        return locate(tree, argv[3])
    if mode == "expect-text":
        return expect_text(tree, argv[3])
    print(f"unknown mode {mode!r}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
