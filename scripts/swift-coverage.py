#!/usr/bin/env python3
"""Convert `llvm-cov export -format=text` JSON for the Swift bridge to Sonar generic XML.

Usage: swift-coverage.py <llvm-cov-export.json> <swift-generic.xml>

Only files under `macos/native/` are emitted with workspace-relative paths,
so absolute runner paths never leak into the report. Each executable line
seen in the export segments is emitted once: covered when any counted
segment starting on that line executed at least once.
"""

import json
import sys

NATIVE_PREFIX = "macos/native/"


def main():
    if len(sys.argv) != 3:
        raise SystemExit("usage: swift-coverage.py <llvm-cov-export.json> <swift-generic.xml>")
    with open(sys.argv[1], encoding="utf-8") as handle:
        export = json.load(handle)
    if not isinstance(export, dict):
        raise SystemExit("invalid llvm-cov export: top-level value must be an object")

    lines = {}
    for group in export.get("data") or []:
        for entry in group.get("files") or []:
            normalized = (entry.get("filename") or "").replace("\\", "/")
            index = normalized.find(NATIVE_PREFIX)
            if index < 0:
                continue
            per_line = lines.setdefault(normalized[index:], {})
            for segment in entry.get("segments") or []:
                # Segment layout: [line, column, count, hasCount, ...].
                # Entries without a count (whitespace, comments, skipped
                # regions) must not count as uncovered lines.
                if len(segment) < 5 or not segment[3]:
                    continue
                line, count = segment[0], segment[2]
                if type(line) is not int or line < 1 or type(count) is not int:
                    continue
                if count > 0:
                    per_line[line] = True
                else:
                    per_line.setdefault(line, False)

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
