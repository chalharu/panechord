#!/usr/bin/env python3
"""AT-SPI2 out-of-process cross-check for the GTK layout golden.

Compares the OS-observed accessible tree against the shared cross-backend
`dump_layout` golden (`tests/golden/layout_exact.json`).

Window position under the WM is not deterministic, so screen extents from
AT-SPI are normalized to window-relative before exact comparison. IDs must
match exactly; geometry must match exactly after normalization. Font/theme/
GTK updates that move geometry must fail here and re-baseline the golden
explicitly (UPDATE_GOLDEN=1 on the golden test).

Requires an isolated Xvfb + dbus session + at-spi2-registryd and
python3-pyatspi. Missing AT-SPI support is an explicit failure (exit 2),
never a silent skip.
"""

import json
import os
import sys

try:
    import pyatspi
except ImportError:
    pyatspi = None


GOLDEN = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "tests",
    "golden",
    "layout_exact.json",
)


def load_golden():
    with open(GOLDEN, encoding="utf-8") as handle:
        return json.loads(handle.read())


def _role_name(node):
    try:
        return node.get_role_name()
    except Exception as error:  # noqa: BLE001 - AT-SPI raises varied runtime errors
        print(f"AT-SPI role failed: {error}", file=sys.stderr)
        return None


def _node_name(node):
    try:
        return node.get_name() or ""
    except Exception as error:  # noqa: BLE001 - AT-SPI raises varied runtime errors
        print(f"AT-SPI name failed: {error}", file=sys.stderr)
        return None


def _extents(node):
    try:
        return node.get_extents(pyatspi.DESKTOP_COORDS)
    except Exception as error:  # noqa: BLE001 - AT-SPI raises varied runtime errors
        print(f"AT-SPI extents failed: {error}", file=sys.stderr)
        return None


def _children(node):
    try:
        count = node.childCount
    except Exception as error:  # noqa: BLE001 - AT-SPI raises varied runtime errors
        print(f"AT-SPI childCount failed: {error}", file=sys.stderr)
        return
    for index in range(count):
        try:
            yield node.getChildAtIndex(index)
        except Exception as error:  # noqa: BLE001 - AT-SPI raises varied runtime errors
            print(f"AT-SPI child failed: {error}", file=sys.stderr)


def _window_name(window):
    try:
        return window.get_name()
    except Exception as error:  # noqa: BLE001 - AT-SPI raises varied runtime errors
        print(f"AT-SPI window name failed: {error}", file=sys.stderr)
        return None


def dump_window(window):
    origin = _extents(window)
    if origin is None:
        return None
    entries = []

    def visit(node):
        name = _node_name(node)
        role = _role_name(node)
        ext = _extents(node)
        if name is None or role is None or ext is None:
            return
        entries.append(
            {
                "id": name,
                "kind": role,
                "x": round(ext.x - origin.x),
                "y": round(ext.y - origin.y),
                "width": round(ext.width),
                "height": round(ext.height),
            }
        )
        for child in _children(node):
            visit(child)

    visit(window)
    entries.sort(key=lambda entry: entry["id"])
    return entries


def try_atspi_snapshot(window_title="layout golden"):
    if pyatspi is None or not os.environ.get("AT_SPI_BUS_ADDRESS"):
        print(
            "AT-SPI2 bus unavailable (need at-spi2-registryd + python3-pyatspi "
            "under dbus-run-session on an isolated Xvfb); no silent skip, "
            "wire this script into CI explicitly",
            file=sys.stderr,
        )
        return None
    desktop = pyatspi.Registry().getDesktop(0)
    for app in desktop:
        for window in app:
            name = _window_name(window)
            if name is None:
                continue
            if window_title in (name or ""):
                return dump_window(window)
    print(f"AT-SPI2 window not found: {window_title!r}", file=sys.stderr)
    return None


def main():
    golden = load_golden()
    print(f"golden controls: {len(golden)} from {GOLDEN}")
    # NOTE: full cross-check needs a live window titled "layout golden".
    # Start one via the golden test binary under test, or extend this script
    # to spawn `panechord-demo --example minimal` with a fixed title.
    snapshot = try_atspi_snapshot()
    if snapshot is None:
        print("E2E ATSPI FAIL: no OS-observed snapshot", file=sys.stderr)
        return 2
    golden_by_id = {entry["id"]: entry for entry in golden}
    observed_by_id = {entry["id"]: entry for entry in snapshot if entry["id"]}
    missing = sorted(set(golden_by_id) - set(observed_by_id))
    extra = sorted(set(observed_by_id) - set(golden_by_id))
    if missing or extra:
        print(
            f"E2E ATSPI FAIL: missing={missing} extra={extra}",
            file=sys.stderr,
        )
        return 1
    for control_id in sorted(golden_by_id):
        expected = golden_by_id[control_id]
        observed = observed_by_id[control_id]
        if observed != expected:
            print(
                f"E2E ATSPI FAIL: {control_id}: expected={expected} observed={observed}",
                file=sys.stderr,
            )
            return 1
    print(f"E2E ATSPI PASS: {len(golden)} controls window-relative exact")
    return 0


if __name__ == "__main__":
    sys.exit(main())
