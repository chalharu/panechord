#!/usr/bin/env python3
"""Unit tests for scripts/swift-coverage.py (run in CI lint leg on Linux)."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "swift_coverage", Path(__file__).resolve().parent / "swift-coverage.py"
)
CONVERTER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CONVERTER)


class SwiftCoverageTest(unittest.TestCase):
    def test_converts_export_json_to_sonar_generic(self):
        export = {
            "data": [
                {
                    "files": [
                        {
                            "filename": "/Users/runner/work/panechord/panechord/macos/native/AppKit.swift",
                            "summary": {},
                            "segments": [
                                [10, 5, 3, True, True, False],
                                [11, 1, 0, True, True, False],
                                # Skipped region: no count, must not
                                # count as an uncovered line.
                                [12, 1, 0, False, False, False],
                            ],
                        },
                        {
                            # Windows-style separators relativize the
                            # same way and merge into the same file.
                            "filename": "C:\\build\\panechord\\macos\\native\\AppKit.swift",
                            "summary": {},
                            "segments": [[20, 1, 1, True, True, False]],
                        },
                        {
                            "filename": "/usr/local/lib/rustlib/src/rust/library/std/src/panic.rs",
                            "summary": {},
                            "segments": [[1, 1, 99, True, True, False]],
                        },
                    ]
                }
            ]
        }
        expected = """<?xml version="1.0" encoding="UTF-8"?>
<coverage version="1">
  <file path="macos/native/AppKit.swift">
    <lineToCover lineNumber="10" covered="true"/>
    <lineToCover lineNumber="11" covered="false"/>
    <lineToCover lineNumber="20" covered="true"/>
  </file>
</coverage>
"""
        with tempfile.TemporaryDirectory() as work:
            source = Path(work) / "export.json"
            target = Path(work) / "generic.xml"
            source.write_text(json.dumps(export), encoding="utf-8")
            CONVERTER.main(str(source), str(target))
            self.assertEqual(target.read_text(encoding="utf-8"), expected)

    def test_rejects_bad_shape(self):
        with self.assertRaises(SystemExit):
            CONVERTER.collect_lines([])


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
