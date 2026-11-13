#!/usr/bin/env python3
"""Regenerate ``fixtures.json`` from the pinned LibreOffice QA corpus.

The whole ``sd/qa/unit/data/pptx`` directory is enumerated through the GitHub
contents API at the pinned commit, so the manifest always reflects upstream
instead of a hand-maintained subset. Every ``.pptx`` blob is downloaded exactly
once into ``target/external/download/<commit>/`` (raw.githubusercontent.com),
hashed with SHA-256, and inspected with :mod:`zipfile` to count slides.

Output is deterministic: fixtures sorted by name, 2-space JSON, trailing
newline. Entries already present in the old manifest keep their ``strict``,
``thresholds`` and ``note`` fields, so promoting or annotating a fixture in
``fixtures.json`` survives a regeneration. Everything new is a diagnostic
(``strict: false``) entry.

Files that are not readable ZIP archives or contain no slides are listed under
``skipped`` with a machine-readable reason and left out of ``files``.

Usage:
    python3 generate_manifest.py [--commit SHA] [--out fixtures.json]
                                 [--download-dir DIR] [--jobs N] [--refresh]

Requires network access to api.github.com and raw.githubusercontent.com.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import io
import json
import posixpath
import re
import sys
import threading
import time
import urllib.error
import urllib.request
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[4]
DEFAULT_MANIFEST = HERE / "fixtures.json"
DEFAULT_COMMIT = "2ddb63be403dcead26b52f657ba869d28785c038"
CORPUS_DIR = "sd/qa/unit/data/pptx"
UPSTREAM = "https://github.com/LibreOffice/core"
LICENSE = "MPL-2.0"
DISTRIBUTION = "Fetched at test time and not distributed with this repository."
RENDER_WIDTH = 1600
DIAGNOSTIC_NOTE = (
    "LibreOffice QA corpus deck; diagnostic corpus entry "
    "(cross-renderer drift expected)"
)
CONTENTS_LIMIT = 1000
SLIDE_XML = re.compile(r"^slide[0-9]+\.xml$")
HEX40 = re.compile(r"^[0-9a-f]{40}$")

API = "https://api.github.com/repos/LibreOffice/core"
RAW = "https://raw.githubusercontent.com/LibreOffice/core"
USER_AGENT = "kitty-pptx-manifest-generator/1.0"
_print_lock = threading.Lock()


def log(message: str) -> None:
    with _print_lock:
        print(message, flush=True)


def request_bytes(url: str, *, attempts: int = 5, timeout: int = 120) -> bytes:
    """GET ``url`` with bounded exponential backoff.

    GitHub returns 403 for rate limiting and transient 5xx for overload; both
    are retried. A final failure is fatal for the manifest so a partial
    listing can never be mistaken for the corpus.
    """
    delay = 1.0
    last: Exception | None = None
    for attempt in range(1, attempts + 1):
        try:
            request = urllib.request.Request(
                url, headers={"User-Agent": USER_AGENT, "Accept": "application/vnd.github+json"}
            )
            with urllib.request.urlopen(request, timeout=timeout) as response:
                return response.read()
        except urllib.error.HTTPError as error:
            last = error
            if error.code in (404, 401):
                raise SystemExit(f"GET {url}: HTTP {error.code} (check the pinned commit)")
            if error.code not in (403, 429) and error.code < 500:
                raise SystemExit(f"GET {url}: HTTP {error.code}")
            retry_after = error.headers.get("Retry-After")
            sleep = float(retry_after) if retry_after and retry_after.isdigit() else delay
        except (urllib.error.URLError, TimeoutError, OSError) as error:
            last = error
            sleep = delay
        if attempt < attempts:
            log(f"  retry {attempt}/{attempts - 1} in {sleep:.1f}s: {url}")
            time.sleep(sleep)
            delay = min(delay * 2, 30.0)
    raise SystemExit(f"GET {url} failed after {attempts} attempts: {last}")


def listing_via_contents(commit: str) -> list[dict]:
    url = f"{API}/contents/{CORPUS_DIR}?ref={commit}"
    payload = json.loads(request_bytes(url))
    if not isinstance(payload, list):
        raise SystemExit(f"{url}: expected a directory listing")
    return payload


def listing_via_tree(commit: str) -> list[dict]:
    """Fallback for directories above the contents API's 1000-entry cap."""
    url = f"{API}/git/trees/{commit}:{CORPUS_DIR}"
    payload = json.loads(request_bytes(url))
    if payload.get("truncated"):
        raise SystemExit(f"{url}: tree listing truncated; refusing an incomplete corpus")
    return [
        {
            "name": posixpath.basename(entry["path"]),
            "type": "file",
            "path": f"{CORPUS_DIR}/{entry['path']}",
        }
        for entry in payload.get("tree", [])
        if entry.get("type") == "blob" and entry["path"].endswith(".pptx")
    ]


def corpus_listing(commit: str) -> list[dict]:
    entries = listing_via_contents(commit)
    if len(entries) >= CONTENTS_LIMIT:
        log(f"contents API returned {len(entries)} entries; using the git tree API")
        entries = listing_via_tree(commit)
    files = []
    for entry in entries:
        if entry.get("type") != "file":
            continue
        name = entry.get("name", "")
        if name.startswith(".") or not name.endswith(".pptx"):
            continue
        path = entry.get("path") or posixpath.join(CORPUS_DIR, name)
        if not path.startswith(CORPUS_DIR + "/"):
            raise SystemExit(f"unexpected corpus path {path!r}")
        files.append({"name": name, "path": path})
    files.sort(key=lambda entry: entry["name"])
    return files


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def download(url: str, target: Path, refresh: bool) -> bytes:
    if target.is_file() and not refresh:
        try:
            data = target.read_bytes()
        except OSError:
            data = b""
        if data:
            return data
    data = request_bytes(url)
    if not data:
        raise SystemExit(f"GET {url}: empty response")
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_suffix(target.suffix + ".tmp")
    temporary.write_bytes(data)
    temporary.replace(target)
    return data


def count_slides(data: bytes) -> tuple[int | None, str | None]:
    """Return ``(slides, reason)``; ``reason`` is set when the deck is skipped."""
    try:
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            names = archive.namelist()
            # Force a CRC pass over the central directory entries we rely on so
            # a truncated upload cannot masquerade as a renderable deck.
            bad = archive.testzip()
    except zipfile.BadZipFile as error:
        return None, f"not a readable zip archive: {error}"
    except (OSError, ValueError, EOFError) as error:
        return None, f"unreadable zip archive: {error}"
    if bad is not None:
        return None, f"corrupt zip member: {bad}"
    slides = sum(
        1
        for name in names
        if posixpath.dirname(name) == "ppt/slides" and SLIDE_XML.fullmatch(posixpath.basename(name))
    )
    if slides == 0:
        return None, "zip contains no ppt/slides/slide*.xml"
    return slides, None


def load_existing(path: Path) -> dict[str, dict]:
    if not path.is_file():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {}
    return {
        entry["name"]: entry
        for entry in data.get("files", [])
        if isinstance(entry, dict) and isinstance(entry.get("name"), str)
    }


def manifest_entry(name: str, path: str, digest: str, slides: int, old: dict) -> dict:
    """Build one entry, inheriting curated metadata for known fixtures."""
    entry = {
        "name": name,
        "path": path,
        "sha256": digest,
        "slides": slides,
        "strict": bool(old.get("strict", False)),
        "note": old.get("note") or DIAGNOSTIC_NOTE,
    }
    if "thresholds" in old:
        entry["thresholds"] = old["thresholds"]
    return entry


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--commit", default=None, help="LibreOffice core commit (40 hex)")
    parser.add_argument("--out", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--download-dir", type=Path, default=None)
    parser.add_argument("--jobs", type=int, default=8, help="concurrent inspections (default 8)")
    parser.add_argument("--refresh", action="store_true", help="re-download cached blobs")
    args = parser.parse_args(argv[1:])

    old_manifest = load_existing(args.out)
    commit = args.commit
    if commit is None:
        if args.out.is_file():
            try:
                commit = json.loads(args.out.read_text(encoding="utf-8")).get("commit")
            except json.JSONDecodeError:
                commit = None
        commit = commit or DEFAULT_COMMIT
    if not HEX40.fullmatch(commit):
        raise SystemExit(f"commit must be 40 lowercase hex characters, got {commit!r}")

    download_dir = args.download_dir or (
        REPO_ROOT / "target" / "external" / "download" / commit
    )
    download_dir.mkdir(parents=True, exist_ok=True)

    log(f"listing {CORPUS_DIR} at {commit}")
    entries = corpus_listing(commit)
    log(f"found {len(entries)} .pptx files")

    # The GitHub contents listing is already sorted; keep processing sorted so
    # progress and skip order are stable. Download serially here: 419 files at
    # ~30 KB average is network-latency bound and a single stream avoids
    # secondary rate limits. ``--jobs`` parallelises the hash/slide pass below.
    started = time.monotonic()
    downloaded: list[tuple[dict, bytes]] = []
    for index, entry in enumerate(entries, 1):
        data = download(
            f"{RAW}/{commit}/{entry['path']}", download_dir / entry["name"], args.refresh
        )
        downloaded.append((entry, data))
        if index % 50 == 0 or index == len(entries):
            log(f"  downloaded {index}/{len(entries)}")
    log(f"downloaded {len(downloaded)} file(s) in {time.monotonic() - started:.1f}s")

    files: list[dict] = []
    skipped: list[dict] = []
    total_bytes = 0
    total_slides = 0

    def inspect(item: tuple[dict, bytes]) -> dict:
        entry, data = item
        stem = Path(entry["name"]).stem
        slides, reason = count_slides(data)
        if reason is not None:
            return {"skipped": {"name": stem, "path": entry["path"], "reason": reason}}
        return {
            "entry": manifest_entry(
                stem, entry["path"], sha256_bytes(data), slides or 0, old_manifest.get(stem, {})
            ),
            "bytes": len(data),
            "slides": slides or 0,
        }

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        for result in pool.map(inspect, downloaded):
            if "skipped" in result:
                skipped.append(result["skipped"])
                log(f"  skipped {result['skipped']['name']}: {result['skipped']['reason']}")
                continue
            files.append(result["entry"])
            total_bytes += result["bytes"]
            total_slides += result["slides"]

    files.sort(key=lambda item: item["name"])
    skipped.sort(key=lambda item: item["name"])

    manifest = {
        "upstream": UPSTREAM,
        "commit": commit,
        "license": LICENSE,
        "distribution": DISTRIBUTION,
        "render_width": RENDER_WIDTH,
        "files": files,
        "skipped": skipped,
    }
    text = json.dumps(manifest, indent=2, ensure_ascii=False) + "\n"
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(text, encoding="utf-8")

    old_strict = sum(1 for name, entry in old_manifest.items() if entry.get("strict"))
    kept = sum(1 for name in old_manifest if any(item["name"] == name for item in files))
    log(
        f"wrote {args.out} ({len(text)} bytes): {len(files)} fixtures "
        f"({sum(1 for item in files if item['strict'])} strict, "
        f"{len(files) - sum(1 for item in files if item['strict'])} diagnostic), "
        f"{total_slides} slides, {total_bytes} bytes; "
        f"{len(skipped)} skipped; {kept}/{len(old_manifest)} previous fixtures kept "
        f"({old_strict} were strict)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
