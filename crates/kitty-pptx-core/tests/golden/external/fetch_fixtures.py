#!/usr/bin/env python3
"""Fetch the pinned external fixtures without cloning the full repository.

The checkout is intentionally narrow:

* ``--depth=1 --filter=tree:0`` fetches only the pinned commit, not its trees;
* non-cone ``sparse-checkout`` lists exactly the manifest paths;
* Git fetches the blobs for those paths only when they are checked out.

The result is verified with SHA-256 and installed atomically into the cache.
Run this through ``run.py`` unless you are debugging the fetch step.

Usage:
    python3 fetch_fixtures.py [--manifest fixtures.json] [--dest DIR] [--force]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
DEFAULT_MANIFEST = HERE / "fixtures.json"
REPO_ROOT = HERE.parents[4]
SAFE_NAME = re.compile(r"^[A-Za-z0-9._-]+$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_manifest(path: Path) -> dict:
    data = json.loads(path.read_text(encoding="utf-8"))
    commit = data.get("commit", "")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise SystemExit(f"{path}: commit must be a 40-character lowercase SHA")
    upstream = data.get("upstream", "")
    if not upstream.startswith("https://"):
        raise SystemExit(f"{path}: upstream must be an https URL")
    files = data.get("files")
    if not isinstance(files, list) or not files:
        raise SystemExit(f"{path}: files must be a non-empty list")

    seen: set[str] = set()
    for entry in files:
        name = entry.get("name", "")
        source = entry.get("path", "")
        digest = entry.get("sha256", "")
        if not SAFE_NAME.fullmatch(name) or name in seen:
            raise SystemExit(f"{path}: invalid or duplicate fixture name {name!r}")
        seen.add(name)
        if not source.startswith("sd/qa/unit/data/pptx/") or ".." in source:
            raise SystemExit(f"{path}: refusing fixture path {source!r}")
        if not HEX64.fullmatch(digest):
            raise SystemExit(f"{path}: invalid sha256 for {name!r}")
        if not isinstance(entry.get("slides"), int) or entry["slides"] < 1:
            raise SystemExit(f"{path}: invalid slide count for {name!r}")
    return data


def run_git(args: list[str], cwd: Path, env: dict[str, str]) -> None:
    try:
        subprocess.run(
            ["git", *args],
            cwd=cwd,
            env=env,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
    except FileNotFoundError as error:
        raise SystemExit("git is required to fetch external fixtures") from error
    except subprocess.CalledProcessError as error:
        output = (error.stdout or "").strip()
        raise SystemExit(
            f"git {' '.join(args)} failed in {cwd}:\n{output}"
        ) from error


def cache_is_valid(dest: Path, files: list[dict]) -> bool:
    for entry in files:
        path = dest / f"{entry['name']}.pptx"
        if not path.is_file() or sha256_file(path) != entry["sha256"]:
            return False
    return True


def fetch(manifest_path: Path, dest: Path, force: bool) -> Path:
    manifest = load_manifest(manifest_path)
    files = manifest["files"]
    commit = manifest["commit"]

    if not force and cache_is_valid(dest, files):
        print(f"fixtures cached: {dest}")
        return dest

    dest.parent.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix=".kitty-pptx-fetch-", dir=dest.parent))
    clone = work / "clone"
    output = work / "output"
    clone.mkdir()
    output.mkdir()
    env = os.environ.copy()
    env["GIT_TERMINAL_PROMPT"] = "0"

    try:
        run_git(["init", "--quiet"], clone, env)
        run_git(["remote", "add", "origin", manifest["upstream"]], clone, env)
        run_git(
            [
                "fetch",
                "--quiet",
                "--depth=1",
                "--filter=tree:0",
                "origin",
                commit,
            ],
            clone,
            env,
        )
        run_git(["sparse-checkout", "init", "--no-cone"], clone, env)
        patterns = "\n".join("/" + entry["path"] for entry in files) + "\n"
        (clone / ".git" / "info" / "sparse-checkout").write_text(
            patterns, encoding="utf-8"
        )
        run_git(["checkout", "--quiet", "FETCH_HEAD"], clone, env)

        total_bytes = 0
        for entry in files:
            source = clone / entry["path"]
            if not source.is_file():
                raise SystemExit(f"checkout did not produce {entry['path']}")
            actual = sha256_file(source)
            if actual != entry["sha256"]:
                raise SystemExit(
                    f"{entry['path']}: sha256 mismatch\n"
                    f"  expected {entry['sha256']}\n"
                    f"  actual   {actual}"
                )
            target = output / f"{entry['name']}.pptx"
            shutil.copyfile(source, target)
            target.chmod(0o644)
            total_bytes += target.stat().st_size

        if dest.exists():
            shutil.rmtree(dest)
        output.rename(dest)
        print(f"fetched {len(files)} fixture(s), {total_bytes} bytes: {dest}")
        return dest
    finally:
        shutil.rmtree(work, ignore_errors=True)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--dest", type=Path)
    parser.add_argument("--force", action="store_true")
    args = parser.parse_args(argv[1:])

    manifest = load_manifest(args.manifest)
    dest = args.dest or (
        REPO_ROOT / "target" / "external" / "fixtures" / manifest["commit"]
    )
    fetch(args.manifest, dest, args.force)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
