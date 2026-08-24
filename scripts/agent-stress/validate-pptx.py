#!/usr/bin/env python3
"""Read-only structural validation for a generated PPTX artifact."""

from __future__ import annotations

import json
import re
import sys
import xml.etree.ElementTree as ET
import zipfile


SLIDE_NAME = re.compile(r"ppt/slides/slide(\d+)\.xml")
REQUIRED_PARTS = {"[Content_Types].xml", "ppt/presentation.xml"}
MAX_XML_BYTES = 16 * 1024 * 1024
MAX_TOTAL_XML_BYTES = 64 * 1024 * 1024


def inspect(path: str, expected: int) -> dict[str, object]:
    with zipfile.ZipFile(path) as deck:
        infos = deck.infolist()
        names = [info.filename for info in infos]
        unique_names = set(names)
        missing = sorted(REQUIRED_PARTS - unique_names)
        duplicates = sorted({name for name in names if names.count(name) > 1})
        slide_infos = sorted(
            (info for info in infos if SLIDE_NAME.fullmatch(info.filename)),
            key=lambda info: int(SLIDE_NAME.fullmatch(info.filename).group(1)),
        )
        xml_infos = [
            info
            for info in infos
            if info.filename in REQUIRED_PARTS or SLIDE_NAME.fullmatch(info.filename)
        ]
        oversized = sorted(info.filename for info in xml_infos if info.file_size > MAX_XML_BYTES)
        total_xml_bytes = sum(info.file_size for info in xml_infos)
        malformed: list[str] = []

        if not oversized and total_xml_bytes <= MAX_TOTAL_XML_BYTES:
            for info in xml_infos:
                try:
                    ET.fromstring(deck.read(info))
                except (ET.ParseError, RuntimeError, ValueError, zipfile.BadZipFile):
                    malformed.append(info.filename)

        ok = (
            not missing
            and not duplicates
            and not oversized
            and total_xml_bytes <= MAX_TOTAL_XML_BYTES
            and not malformed
            and len(slide_infos) == expected
        )
        return {
            "ok": ok,
            "slides": len(slide_infos),
            "expected": expected,
            "missing": missing,
            "duplicates": duplicates,
            "oversized": oversized,
            "malformed": sorted(malformed),
            "xmlBytes": total_xml_bytes,
        }


def main() -> int:
    if len(sys.argv) != 3:
        print(json.dumps({"ok": False, "error": "usage: validate-pptx.py PATH EXPECTED"}))
        return 2
    try:
        expected = int(sys.argv[2])
        if expected <= 0:
            raise ValueError("expected slide count must be positive")
        result = inspect(sys.argv[1], expected)
    except (OSError, ValueError, zipfile.BadZipFile, zipfile.LargeZipFile) as error:
        result = {"ok": False, "error": f"{type(error).__name__}: {error}"}
    print(json.dumps(result, ensure_ascii=False, separators=(",", ":")))
    return 0 if result.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
