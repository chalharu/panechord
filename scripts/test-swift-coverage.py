#!/usr/bin/env python3
"""Unit tests for scripts/swift-coverage.py (run in CI lint leg on Linux)."""

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent


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
                            ],
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
        with tempfile.TemporaryDirectory() as work:
            source = Path(work) / "export.json"
            target = Path(work) / "generic.xml"
            source.write_text(json.dumps(export), encoding="utf-8")
            subprocess.run(
                ["python3", str(ROOT / "swift-coverage.py"), str(source), str(target)],
                check=True,
            )
            self.assertEqual(
                target.read_text(encoding="utf-8"),
                '<?xml version="1.0" encoding="UTF-8"?>\n'
                "<coverage version=\"1\">\n"
                '  <file path="macos/native/AppKit.swift">\n'
                '    <lineToCover lineNumber="10" covered="true"/>\n'
                '    <lineToCover lineNumber="11" covered="false"/>\n'
                "  </file>\n"
                "</coverage>\n",
            )


if __name__ == "__main__":
    unittest.main()
