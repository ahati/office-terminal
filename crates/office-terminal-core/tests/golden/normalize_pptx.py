#!/usr/bin/env python3
"""Rewrite a PptxGenJS package into a byte-stable golden fixture.

PptxGenJS stamps wall-clock dates into ``docProps/core.xml``, embedded chart
workbooks, and ZIP entry headers. Two runs of the same generator therefore
produce different bytes. This script normalizes the archive recursively:

* every ZIP entry gets the fixed timestamp 1980-01-01 00:00:00,
* ``dcterms:created`` / ``dcterms:modified`` are pinned,
* entries are written in sorted order with deterministic compression.

Embedded ``.xlsx`` workbooks are normalized recursively, including their own
core properties. The result is safe to commit and can be regenerated exactly.

Usage:
    python3 normalize_pptx.py <input.pptx> [<output.pptx>]
"""

from __future__ import annotations

import io
import os
import re
import sys
import tempfile
import zipfile
from pathlib import Path

FIXED_ZIP_DATE = (1980, 1, 1, 0, 0, 0)
FIXED_W3C = b"2000-01-01T00:00:00Z"
CORE_DATE = re.compile(
    rb"(<dcterms:(?:created|modified)\b[^>]*>).*?(</dcterms:(?:created|modified)>)",
    re.DOTALL,
)
NESTED_ZIP_SUFFIXES = (".xlsx", ".xlsm", ".docx", ".docm", ".pptx", ".pptm")


def normalize_core_xml(data: bytes) -> bytes:
    """Pin the two W3CDTF timestamps used by OOXML core properties."""
    return CORE_DATE.sub(rb"\1" + FIXED_W3C + rb"\2", data)


def normalize_zip_bytes(data: bytes) -> bytes:
    """Normalize one ZIP archive, recursing into embedded OOXML packages."""
    with zipfile.ZipFile(io.BytesIO(data), "r") as source:
        entries: dict[str, tuple[bytes, bool]] = {}
        for info in source.infolist():
            payload = source.read(info)
            is_dir = info.filename.endswith("/")
            if not is_dir:
                if info.filename.endswith(NESTED_ZIP_SUFFIXES):
                    payload = normalize_zip_bytes(payload)
                elif info.filename == "docProps/core.xml":
                    payload = normalize_core_xml(payload)
            entries[info.filename] = (payload, is_dir)

    output = io.BytesIO()
    with zipfile.ZipFile(
        output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as target:
        for name in sorted(entries):
            payload, is_dir = entries[name]
            info = zipfile.ZipInfo(name, FIXED_ZIP_DATE)
            info.create_system = 3  # Unix attributes, stable across hosts
            info.external_attr = (0o40755 if is_dir else 0o100644) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            target.writestr(info, payload)
    return output.getvalue()


def main(argv: list[str]) -> int:
    if len(argv) not in (2, 3):
        print(__doc__.strip(), file=sys.stderr)
        return 2

    source = Path(argv[1])
    target = Path(argv[2]) if len(argv) == 3 else source
    normalized = normalize_zip_bytes(source.read_bytes())

    handle, temporary = tempfile.mkstemp(
        prefix=target.name + ".", suffix=".tmp", dir=target.parent
    )
    try:
        with os.fdopen(handle, "wb") as stream:
            stream.write(normalized)
        os.replace(temporary, target)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
