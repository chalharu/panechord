#!/usr/bin/env python3
"""Convert `llvm-cov export -format=text` JSON for the Swift bridge to Sonar generic XML.

Only files under `macos/native/` are emitted with workspace-relative paths,
so absolute runner paths never leak into the report. Each executable line
seen in the export segments is emitted once: covered when any counted
segment starting on that line executed at least once.
"""

import argparse
import json

NATIVE_PREFIX = "macos/native/"


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("export_json", help="llvm-cov export JSON input")
    parser.add_argument("generic_xml", help="Sonar generic XML output")
    return parser.parse_args(argv)


def load_export(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def relative_native_path(filename):
    normalized = (filename or "").replace("\\", "/")
    index = normalized.find(NATIVE_PREFIX)
    return None if index < 0 else normalized[index:]


def segment_cover(segment):
    """Map one export segment to (line, covered), or None when uncounted."""
    # Segment layout: [line, column, count, hasCount, ...]. Entries without
    # a count (whitespace, comments, skipped regions) must not count as
    # uncovered lines.
    if len(segment) < 5 or not segment[3]:
        return None
    line, count = segment[0], segment[2]
    if type(line) is not int or line < 1 or type(count) is not int:
        return None
    return line, count > 0


def collect_lines(export):
    if not isinstance(export, dict):
        raise SystemExit("invalid llvm-cov export: top-level value must be an object")
    lines = {}
    for group in export.get("data") or []:
        for entry in group.get("files") or []:
            relative = relative_native_path(entry.get("filename"))
            if relative is None:
                continue
            per_line = lines.setdefault(relative, {})
            for segment in entry.get("segments") or []:
                covered = segment_cover(segment)
                if covered is None:
                    continue
                line, hit = covered
                if hit:
                    per_line[line] = True
                else:
                    per_line.setdefault(line, False)
    return lines


def write_generic_xml(lines, path):
    with open(path, "w", encoding="utf-8", newline="\n") as out:
        out.write('<?xml version="1.0" encoding="UTF-8"?>\n<coverage version="1">\n')
        for relative in sorted(lines):
            out.write(f'  <file path="{relative}">\n')
            for line in sorted(lines[relative]):
                value = "true" if lines[relative][line] else "false"
                out.write(f'    <lineToCover lineNumber="{line}" covered="{value}"/>\n')
            out.write("  </file>\n")
        out.write("</coverage>\n")


def main(argv=None):
    args = parse_args(argv)
    write_generic_xml(collect_lines(load_export(args.export_json)), args.generic_xml)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
