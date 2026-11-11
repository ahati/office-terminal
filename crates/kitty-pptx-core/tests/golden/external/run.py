#!/usr/bin/env python3
"""Recreate and run the opt-in external renderer comparison.

One command:

    python3 crates/kitty-pptx-core/tests/golden/external/run.py

The expensive work is cached under ``target/external``:

* ``fixtures/<commit>/``  — pinned LibreOffice decks fetched by sparse checkout
* ``goldens/<commit>/``   — PNGs rendered by office-open-xml-viewer/Playwright

A stamp records the manifest, renderer scripts, and pinned viewer versions. If
nothing changed, subsequent runs skip fetching and rendering and go straight to
the Rust comparison. Use ``--force`` to rebuild both caches.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[4]
MANIFEST = HERE / "fixtures.json"
FETCH = HERE / "fetch_fixtures.py"
VIEWER = HERE / "viewer"
CACHE = REPO_ROOT / "target" / "external"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run(command: list[str], cwd: Path | None = None, env: dict[str, str] | None = None) -> None:
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=cwd, env=env, check=True)


def load_manifest() -> dict:
    return json.loads(MANIFEST.read_text(encoding="utf-8"))


def package_version(package: str) -> str:
    data = json.loads((VIEWER / "package.json").read_text(encoding="utf-8"))
    return data["dependencies"][package]


def installed_version(package: str) -> str | None:
    path = VIEWER / "node_modules" / package / "package.json"
    if not path.is_file():
        return None
    return json.loads(path.read_text(encoding="utf-8")).get("version")


def ensure_viewer(offline: bool) -> None:
    wanted_ooxml = package_version("@silurus/ooxml")
    wanted_playwright = package_version("playwright")
    if (
        installed_version("@silurus/ooxml") != wanted_ooxml
        or installed_version("playwright") != wanted_playwright
    ):
        if offline:
            raise SystemExit(
                "viewer dependencies are missing and --offline was requested; "
                f"run `npm ci` in {VIEWER}"
            )
        npm = shutil.which("npm")
        if not npm:
            raise SystemExit("npm is required to install the external viewer")
        run([npm, "ci", "--ignore-scripts", "--no-audit", "--no-fund"], cwd=VIEWER)
    else:
        print(f"viewer dependencies cached: @silurus/ooxml {wanted_ooxml}")

    if offline:
        print("playwright browser check skipped (--offline)")
        return
    npx = shutil.which("npx")
    if not npx:
        raise SystemExit("npx is required to install the Playwright browser")
    run([npx, "playwright", "install", "chromium"], cwd=VIEWER)


def reference_stamp(manifest: dict) -> dict:
    return {
        "commit": manifest["commit"],
        "render_width": manifest["render_width"],
        "manifest_sha256": sha256_file(MANIFEST),
        "viewer": {
            "@silurus/ooxml": package_version("@silurus/ooxml"),
            "playwright": package_version("playwright"),
        },
        "renderer_sha256": {
            name: sha256_file(VIEWER / name)
            for name in ("index.html", "render.mjs", "serve.mjs")
        },
        "fixtures": [
            {
                "name": entry["name"],
                "sha256": entry["sha256"],
                "slides": entry["slides"],
            }
            for entry in manifest["files"]
        ],
    }


def references_complete(goldens: Path, manifest: dict) -> bool:
    for entry in manifest["files"]:
        directory = goldens / entry["name"]
        if not directory.is_dir():
            return False
        slides = sorted(directory.glob("slide_*.png"))
        if len(slides) != entry["slides"] or any(
            slide.stat().st_size == 0 for slide in slides
        ):
            return False
    return True


def references_ready(goldens: Path, manifest: dict, stamp: dict) -> bool:
    stamp_path = goldens / "stamp.json"
    if not stamp_path.is_file():
        return False
    try:
        if json.loads(stamp_path.read_text(encoding="utf-8")) != stamp:
            return False
    except json.JSONDecodeError:
        return False
    return references_complete(goldens, manifest)


def write_stamp(goldens: Path, stamp: dict) -> None:
    temporary = goldens / "stamp.json.tmp"
    temporary.write_text(json.dumps(stamp, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    temporary.replace(goldens / "stamp.json")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--force", action="store_true", help="refetch and rerender")
    parser.add_argument("--force-fetch", action="store_true")
    parser.add_argument("--force-render", action="store_true")
    parser.add_argument("--skip-fetch", action="store_true")
    parser.add_argument("--skip-render", action="store_true")
    parser.add_argument("--skip-test", action="store_true")
    parser.add_argument("--offline", action="store_true", help="use existing caches only")
    args = parser.parse_args(argv[1:])

    manifest = load_manifest()
    commit = manifest["commit"]
    fixtures = CACHE / "fixtures" / commit
    goldens = CACHE / "goldens" / commit
    stamp = reference_stamp(manifest)

    print(f"external fixtures: {fixtures}")
    if args.skip_fetch:
        if not fixtures.is_dir():
            raise SystemExit(f"fixtures are missing at {fixtures}")
        print("fetch skipped (--skip-fetch)")
    else:
        command = [sys.executable, str(FETCH), "--manifest", str(MANIFEST), "--dest", str(fixtures)]
        if args.force or args.force_fetch:
            command.append("--force")
        run(command)

    ensure_viewer(args.offline)

    if args.skip_render:
        print("reference render skipped (--skip-render)")
    elif not (args.force or args.force_render) and references_ready(goldens, manifest, stamp):
        print(f"references cached: {goldens}")
    else:
        if goldens.exists():
            shutil.rmtree(goldens)
        goldens.mkdir(parents=True, exist_ok=True)
        node = shutil.which("node")
        if not node:
            raise SystemExit("node is required to render external references")
        run(
            [
                node,
                "render.mjs",
                "--manifest",
                str(MANIFEST),
                "--fixtures",
                str(fixtures),
                "--out",
                str(goldens),
                "--width",
                str(manifest["render_width"]),
            ],
            cwd=VIEWER,
        )
        if not references_complete(goldens, manifest):
            raise SystemExit("reference render did not produce the expected files")
        write_stamp(goldens, stamp)
        print(f"references written: {goldens}")

    if args.skip_test:
        print("comparison skipped (--skip-test)")
        return 0

    env = os.environ.copy()
    env["KITTY_PPTX_EXTERNAL_FIXTURES"] = str(fixtures)
    env["KITTY_PPTX_EXTERNAL_GOLDENS"] = str(goldens)
    run(
        [
            "cargo",
            "test",
            "-p",
            "kitty-pptx-core",
            "--test",
            "external_fixture_tests",
            "--",
            "--ignored",
            "--nocapture",
        ],
        cwd=REPO_ROOT,
        env=env,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
