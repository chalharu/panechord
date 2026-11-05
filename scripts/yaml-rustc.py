#!/usr/bin/env python3
"""Preserve rustc diagnostics, adding primary YAML spans from node expansions."""
import json
import pathlib
import re
import subprocess
import sys

MARKER = re.compile(r"__panechord_yaml_line_(\d+)_column_(\d+)_file_([0-9a-f]+)")


def resolve_marker(diagnostic):
    """Find the single YAML node marker for this diagnostic, if unambiguous."""
    matches = []
    for span in diagnostic.get("spans", []):
        if not span.get("is_primary"):
            continue
        expansion = span.get("expansion")
        while expansion:
            match = MARKER.fullmatch(expansion.get("macro_decl_name", "").removesuffix("!"))
            if match:
                matches.append(match)
                break
            expansion = expansion.get("span", {}).get("expansion")
    if not matches:
        matches = list(MARKER.finditer(diagnostic.get("message", "")))
    identities = {match.group(0) for match in matches}
    return matches[0] if len(identities) == 1 else None


def locate_node(match):
    """Decode a marker into file/line/column byte offsets, or None."""
    row, column = int(match[1]), int(match[2])
    if row < 1 or column < 1:
        return None
    try:
        filename = bytes.fromhex(match[3]).decode("utf-8")
        lines = pathlib.Path(filename).read_bytes().decode("utf-8").splitlines(keepends=True)
        text = lines[row - 1].rstrip("\r\n")
    except (ValueError, OSError, IndexError):
        return None
    if column > len(text):
        return None
    before = "".join(lines[:row - 1]) + text[:column - 1]
    start = len(before.encode("utf-8"))
    end = start + len(text[column - 1:column].encode("utf-8"))
    return filename, row, column, text, start, end


def decorate(diagnostic):
    match = resolve_marker(diagnostic)
    if not match or diagnostic.get("level") != "error":
        return diagnostic
    located = locate_node(match)
    if not located:
        return diagnostic
    filename, row, column, text, start, end = located
    for span in diagnostic.get("spans", []):
        span["is_primary"] = False
    diagnostic.setdefault("spans", []).insert(0, {
        "file_name": filename, "byte_start": start, "byte_end": end,
        "line_start": row, "line_end": row, "column_start": column,
        "column_end": column + 1, "is_primary": True,
        "text": [{"text": text, "highlight_start": column, "highlight_end": column + 1}],
        "label": "YAML node", "suggested_replacement": None,
        "suggestion_applicability": None, "expansion": None,
    })
    code = diagnostic.get("code") or {}
    heading = "error" + (f"[{code['code']}]" if code.get("code") else "")
    original = diagnostic.get("rendered") or ""
    diagnostic["rendered"] = (
        f"{heading}: {diagnostic['message']}\n"
        f" --> {filename}:{row}:{column}\n"
        f"  |\n{row:>2} | {text}\n  | {' ' * (column - 1)}^ YAML node\n"
        f"\n{original}"
    )
    return diagnostic


def main():
    process = subprocess.Popen(sys.argv[1:], stderr=subprocess.PIPE, close_fds=False)
    for raw in process.stderr:
        try:
            diagnostic = json.loads(raw)
            if diagnostic.get("$message_type") == "diagnostic":
                raw = (json.dumps(decorate(diagnostic), ensure_ascii=False) + "\n").encode("utf-8")
        except (ValueError, AttributeError):
            pass
        sys.stderr.buffer.write(raw)
        sys.stderr.buffer.flush()
    return process.wait()


if __name__ == "__main__":
    sys.exit(main())