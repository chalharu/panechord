#!/usr/bin/env python3
"""Convert `llvm-cov export -format=text` JSON for the Swift bridge to Sonar generic XML.

Usage: swift-coverage.py <llvm-cov-export.json> <swift-generic.xml>

Only files under `macos/native/` are emitted with workspace-relative paths,
so absolute runner paths never leak into the report. Each source line seen in
the export segments is emitted once: covered when any segment starting on
that line executed at least once. `AppKitTest.swift` may appear here, but
`sonar.coverage.exclusions` already drops the test-only bridge.
"""

import json
import sys


def main():
    if len(sys.argv) != 3:
        raise SystemExit("usage: swift-coverage.py <llvm-cov-export.json> <swift-generic.xml>")
    with open(sys.argv[1], encoding="utf-8") as handle:
        export = json.load(handle)

    lines = {}
    for group in export.get("data", []):
        for entry in group.get("files", []):
            filename = entry.get("filename", "")
            marker = "macos/native/"
            index = filename.replace("\\", "/").find(marker)
            if index < 0:
                continue
            relative = filename.replace("\\", "/")[index:]
            covered = lines.setdefault(relative, {})
            for segment in entry.get("segments", []):
                if len(segment) < 3:
                    continue
                line, count = segment[0], segment[2]
                if not isinstance(line, int) or line < 1:
                    continue
                if not isinstance(count, int):
                    continue
                if count > 0:
                    covered[line] = True
                else:
                    covered.setdefault(line, False)

    with open(sys.argv[2], "w", encoding="utf-8", newline="\n") as out:
        out.write('<?xml version="1.0" encoding="UTF-8"?>\n<coverage version="1">\n')
        for relative in sorted(lines):
            out.write(f'  <file path="{relative}">\n')
            for line in sorted(lines[relative]):
                value = "true" if lines[relative][line] else "false"
                out.write(f'    <lineToCover lineNumber="{line}" covered="{value}"/>\n')
            out.write("  </file>\n")
        out.write("</coverage>\n")


if __name__ == "__main__":
    main()
