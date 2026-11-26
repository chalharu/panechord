#!/usr/bin/env python3
"""AT-SPI2 out-of-process cross-check for the GTK layout golden.

Compares the OS-observed accessible tree against the in-process
`dump_layout` golden (`gtk/tests/golden/layout_exact.json`).

Window position under the WM is not deterministic, so screen extents from
AT-SPI are normalized to window-relative before exact comparison. IDs and
roles must match exactly; geometry must match exactly after normalization.
Font/theme/GTK updates that move geometry must fail here and re-baseline
the golden explicitly (UPDATE_GOLDEN=1 on the golden test).

Requires an isolated Xvfb + dbus session + at-spi2-registryd and
python3-pyatspi. Missing AT-SPI support is an explicit failure (exit 2),
never a silent skip.
"""
import json
import os
import subprocess
import sys

GOLDEN = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "gtk",
    "tests",
    "golden",
    "layout_exact.json",
)


def load_golden():
    with open(GOLDEN, encoding="utf-8") as handle:
        return json.loads(handle.read())


def try_atspi_snapshot(window_title="layout golden"):
    if not os.environ.get("AT_SPI_BUS_ADDRESS"):
        print(
            "AT-SPI2 bus unavailable (AT_SPI_BUS_ADDRESS unset): run under "
            "dbus-run-session with at-spi2-registryd on an isolated Xvfb; "
            "no silent skip, wire this script into CI explicitly",
            file=sys.stderr,
        )
        return None
    try:
        import pyatspi  # type: ignore
    except ImportError:
        print(
            "AT-SPI2 probe unavailable: install at-spi2-core + python3-pyatspi "
            "and run under dbus-run-session with at-spi2-registryd",
            file=sys.stderr,
        )
        return None
    registry = pyatspi.Registry()
    desktop = registry.getDesktop(0)
    for app in desktop:
        for window in app:
            try:
                name = window.get_name()
            except Exception:
                continue
            if window_title in (name or ""):
                return dump_window(window)
    print(f"AT-SPI2 window not found: {window_title!r}", file=sys.stderr)
    return None


def dump_window(window):
    try:
        origin = window.get_extents(pyatspi.DESKTOP_COORDS)
    except Exception as error:
        print(f"AT-SPI2 extents failed: {error}", file=sys.stderr)
        return None
    entries = []

    def visit(node):
        try:
            role = node.get_role_name()
            name = node.get_name() or ""
            ext = node.get_extents(pyatspi.DESKTOP_COORDS)
        except Exception:
            return
        entries.append(
            {
                "id": name,
                "kind": role,
                "x": int(round(ext.x - origin.x)),
                "y": int(round(ext.y - origin.y)),
                "width": int(round(ext.width)),
                "height": int(round(ext.height)),
            }
        )
        for i in range(node.childCount):
            visit(node.getChildAtIndex(i))

    visit(window)
    entries.sort(key=lambda entry: entry["id"])
    return entries


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
        print(f"E2E ATSPI FAIL: missing={missing} extra={extra}", file=sys.stderr)
        return 1
    for control_id, expected in sorted(golden_by_id.items()):
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
