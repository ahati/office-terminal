#!/usr/bin/env python3
"""Recreate and run the opt-in external renderer comparison.

One command:

    python3 crates/kitty-pptx-core/tests/golden/external/run.py

The expensive work is cached under ``target/external``:

* ``fixtures/<commit>/``  — pinned LibreOffice decks fetched by sparse checkout
* ``goldens/<commit>/``   — PNGs rendered by office-open-xml-viewer/Playwright

Reference rendering is resumable: a deck whose ``slide_*.png`` files all exist
is skipped, and freshly rendered slides are merged one file at a time so an
existing ``slide-*.png`` is never overwritten. Decks the reference viewer
rejects (LibreOffice ships fuzz/malformed samples) are recorded in
``goldens/<commit>/unrenderable.json`` and not retried. ``--limit N`` restricts
the run to the first N manifest entries for smoke tests and prints the measured
render rate plus an extrapolated full-corpus estimate.

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
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[4]
MANIFEST = HERE / "fixtures.json"
FETCH = HERE / "fetch_fixtures.py"
VIEWER = HERE / "viewer"
CACHE = REPO_ROOT / "target" / "external"
BATCH_SIZE = 32
FAILURES = "unrenderable.json"


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


def positive(value: str) -> int:
    number = int(value)
    if number < 1:
        raise argparse.ArgumentTypeError("must be at least 1")
    return number


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


def renderer_identity(manifest: dict) -> dict:
    """Settings that invalidate every cached reference when they change."""
    return {
        "commit": manifest["commit"],
        "render_width": manifest["render_width"],
        "viewer": {
            "@silurus/ooxml": package_version("@silurus/ooxml"),
            "playwright": package_version("playwright"),
        },
        "renderer_sha256": {
            name: sha256_file(VIEWER / name)
            for name in ("index.html", "render.mjs", "serve.mjs")
        },
    }


def reference_stamp(manifest: dict) -> dict:
    stamp = {"manifest_sha256": sha256_file(MANIFEST)}
    stamp.update(renderer_identity(manifest))
    stamp["fixtures"] = [
        {"name": entry["name"], "sha256": entry["sha256"], "slides": entry["slides"]}
        for entry in manifest["files"]
    ]
    return stamp


def slide_names(entry: dict) -> list[str]:
    return [f"slide_{index:03}.png" for index in range(1, entry["slides"] + 1)]


def fixture_is_complete(goldens: Path, entry: dict) -> bool:
    directory = goldens / entry["name"]
    for name in slide_names(entry):
        png = directory / name
        if not png.is_file() or png.stat().st_size == 0:
            return False
    return True


def references_complete(goldens: Path, files: list[dict]) -> bool:
    return all(fixture_is_complete(goldens, entry) for entry in files)


def pending_fixtures(goldens: Path, files: list[dict]) -> list[dict]:
    return [entry for entry in files if not fixture_is_complete(goldens, entry)]


def read_stamp(goldens: Path) -> dict | None:
    path = goldens / "stamp.json"
    if not path.is_file():
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return None


def read_failures(goldens: Path) -> dict[str, str]:
    path = goldens / FAILURES
    if not path.is_file():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {}
    return {str(key): str(value) for key, value in data.items()}


def write_failures(goldens: Path, failures: dict[str, str]) -> None:
    path = goldens / FAILURES
    if not failures:
        path.unlink(missing_ok=True)
        return
    text = json.dumps(dict(sorted(failures.items())), indent=2) + "\n"
    temporary = path.with_suffix(".json.tmp")
    temporary.write_text(text, encoding="utf-8")
    temporary.replace(path)


def stamp_is_current(stamp: dict, manifest: dict) -> bool:
    identity = renderer_identity(manifest)
    if any(stamp.get(key) != value for key, value in identity.items()):
        return False
    entries = stamp.get("fixtures") or []
    wanted = {
        entry["name"]: (entry["sha256"], entry["slides"]) for entry in manifest["files"]
    }
    return len(entries) == len(wanted) and all(
        wanted.get(entry.get("name")) == (entry.get("sha256"), entry.get("slides"))
        for entry in entries
    )


def invalidate_stale(goldens: Path, manifest: dict, stamp: dict | None) -> int:
    """Drop references produced by a different viewer or fixture revision.

    Returns the number of fixture directories removed. A changed manifest that
    only *adds* fixtures keeps existing pixels, which is what makes resumable
    batch rendering useful; any renderer/width/commit change wipes everything.
    """
    if stamp is None:
        return 0
    identity = renderer_identity(manifest)
    if any(stamp.get(key) != value for key, value in identity.items()):
        print("renderer settings changed; discarding cached references")
        for child in goldens.iterdir() if goldens.is_dir() else []:
            if child.is_dir():
                shutil.rmtree(child)
            else:
                child.unlink()
        return len(manifest["files"])
    wanted = {
        entry["name"]: (entry["sha256"], entry["slides"]) for entry in manifest["files"]
    }
    removed = 0
    for entry in stamp.get("fixtures") or []:
        name = entry.get("name")
        directory = goldens / str(name)
        if wanted.get(name) != (entry.get("sha256"), entry.get("slides")) and directory.is_dir():
            print(f"fixture {name} changed; discarding its references")
            shutil.rmtree(directory)
            removed += 1
    return removed


def merge_slides(scratch: Path, goldens: Path, entries: list[dict]) -> tuple[int, int]:
    """Move freshly rendered slides into the cache without clobbering existing ones."""
    written = 0
    skipped = 0
    for entry in entries:
        source = scratch / entry["name"]
        target = goldens / entry["name"]
        target.mkdir(parents=True, exist_ok=True)
        for name in slide_names(entry):
            png = source / name
            if not png.is_file() or png.stat().st_size == 0:
                raise SystemExit(f"renderer produced no usable {png}")
            destination = target / name
            if destination.is_file() and destination.stat().st_size > 0:
                skipped += 1
                continue
            png.replace(destination)
            written += 1
    return written, skipped


def failure_reason(output: str) -> str:
    lines = [line.strip() for line in output.splitlines() if line.strip()]
    for line in lines:
        if line.startswith("Error:"):
            return line[:300]
    return (lines[-1] if lines else "renderer exited non-zero")[:300]


def invoke_renderer(
    entries: list[dict],
    manifest: dict,
    fixtures: Path,
    scratch: Path,
    node: str,
    timeout: float,
) -> tuple[bool, str, str]:
    shutil.rmtree(scratch, ignore_errors=True)
    scratch.mkdir(parents=True)
    subset = {
        "commit": manifest["commit"],
        "render_width": manifest["render_width"],
        "files": entries,
    }
    manifest_path = scratch / "manifest.json"
    manifest_path.write_text(json.dumps(subset, indent=2) + "\n", encoding="utf-8")
    command = [
        node,
        "render.mjs",
        "--manifest",
        str(manifest_path),
        "--fixtures",
        str(fixtures),
        "--out",
        str(scratch / "out"),
        "--width",
        str(manifest["render_width"]),
    ]
    try:
        completed = subprocess.run(
            command,
            cwd=VIEWER,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as error:
        return False, "", f"timeout after {timeout:.0f}s: {error}"
    if completed.returncode != 0:
        return False, completed.stdout or "", failure_reason(completed.stdout or "")
    return True, completed.stdout or "", ""


def render_entries(
    entries: list[dict],
    manifest: dict,
    fixtures: Path,
    goldens: Path,
    failures: dict[str, str],
    scratch: Path,
    node: str,
) -> tuple[int, int, int, int]:
    """Render a batch, splitting on failure so one bad deck cannot abort the run."""
    written = 0
    skipped = 0
    rendered_decks = 0
    rendered_slides = 0
    if not entries:
        return written, skipped, rendered_decks, rendered_slides
    slides = sum(entry["slides"] for entry in entries)
    timeout = max(180.0, 90.0 + 15.0 * slides)
    ok, _output, reason = invoke_renderer(entries, manifest, fixtures, scratch, node, timeout)
    if ok:
        batch_written, batch_skipped = merge_slides(scratch / "out", goldens, entries)
        written += batch_written
        skipped += batch_skipped
        rendered_decks += len(entries)
        rendered_slides += slides
        print(
            f"  rendered {len(entries)} deck(s), {slides} slide(s) "
            f"[{batch_written} new PNG, {batch_skipped} kept]",
            flush=True,
        )
    elif len(entries) == 1:
        name = entries[0]["name"]
        failures[name] = reason
        print(f"  unrenderable {name}: {reason}", flush=True)
        write_failures(goldens, failures)
    else:
        print(
            f"  batch of {len(entries)} deck(s) failed ({reason}); bisecting",
            flush=True,
        )
        middle = len(entries) // 2
        for half in (entries[:middle], entries[middle:]):
            more = render_entries(half, manifest, fixtures, goldens, failures, scratch, node)
            written += more[0]
            skipped += more[1]
            rendered_decks += more[2]
            rendered_slides += more[3]
    shutil.rmtree(scratch, ignore_errors=True)
    return written, skipped, rendered_decks, rendered_slides


def report_rate(started: float, decks: int, slides: int, manifest: dict, remaining: int) -> None:
    elapsed = time.monotonic() - started
    if decks == 0 or elapsed <= 0:
        return
    decks_per_minute = decks * 60.0 / elapsed
    slides_per_minute = slides * 60.0 / elapsed
    total_decks = len(manifest["files"])
    total_slides = sum(entry["slides"] for entry in manifest["files"])
    estimate = total_decks / decks_per_minute * 60.0
    remaining_estimate = remaining / decks_per_minute * 60.0
    print(
        f"render rate: {decks} deck(s) ({slides} slide(s)) in {elapsed:.1f}s "
        f"= {decks_per_minute:.1f} decks/min, {slides_per_minute:.1f} slides/min"
    )
    print(
        f"estimate: {remaining} deck(s) still pending ~ {remaining_estimate / 60:.1f} min; "
        f"full corpus of {total_decks} deck(s)/{total_slides} slide(s) from scratch "
        f"~ {estimate / 60:.1f} min"
    )
    if decks < 10:
        print("estimate: small sample (<10 decks); treat the extrapolation as indicative only")


def selected_files(manifest: dict, limit: int | None) -> list[dict]:
    files = manifest["files"]
    return files if limit is None else files[:limit]


def needs_render(manifest: dict, goldens: Path, limit: int | None) -> bool:
    """True when the stamp is stale or any selected deck lacks a complete set."""
    failures = read_failures(goldens)
    renderable = [
        entry
        for entry in selected_files(manifest, limit)
        if entry["name"] not in failures
    ]
    return not stamp_is_current(read_stamp(goldens) or {}, manifest) or not references_complete(
        goldens, renderable
    )


def render_references(
    manifest: dict, fixtures: Path, goldens: Path, limit: int | None, force: bool
) -> None:
    files = manifest["files"]
    full_corpus = limit is None or limit >= len(files)
    if limit is not None:
        files = files[:limit]
        print(f"limiting the corpus to the first {len(files)} manifest entries")

    goldens.mkdir(parents=True, exist_ok=True)
    stamp = read_stamp(goldens)
    failures = {} if force else read_failures(goldens)
    if force:
        for child in goldens.iterdir():
            if child.is_dir():
                shutil.rmtree(child)
        print("reference cache cleared (--force-render)")
    else:
        invalidate_stale(goldens, manifest, stamp)
    if failures:
        print(f"known unrenderable decks: {len(failures)} (recorded in {FAILURES})")

    renderable = [entry for entry in files if entry["name"] not in failures]
    pending = pending_fixtures(goldens, renderable)
    already = len(renderable) - len(pending)
    print(
        f"references: {len(files)} selected, {already} cached, {len(pending)} pending "
        f"({sum(entry['slides'] for entry in pending)} slide(s))"
    )
    if not pending:
        if (
            full_corpus
            and references_complete(goldens, renderable)
            and not stamp_is_current(read_stamp(goldens) or {}, manifest)
        ):
            write_stamp(goldens, reference_stamp(manifest))
            print("all references present; stamp rewritten")
        else:
            print(f"references cached: {goldens}")
        return

    node = shutil.which("node")
    if not node:
        raise SystemExit("node is required to render external references")
    scratch = goldens / ".pending-render"
    started = time.monotonic()
    written = 0
    skipped = 0
    rendered_decks = 0
    rendered_slides = 0
    batches = [
        pending[index:index + BATCH_SIZE] for index in range(0, len(pending), BATCH_SIZE)
    ]
    for index, batch in enumerate(batches, 1):
        print(
            f"batch {index}/{len(batches)}: {len(batch)} deck(s), "
            f"{sum(entry['slides'] for entry in batch)} slide(s)",
            flush=True,
        )
        more = render_entries(batch, manifest, fixtures, goldens, failures, scratch, node)
        written += more[0]
        skipped += more[1]
        rendered_decks += more[2]
        rendered_slides += more[3]
    shutil.rmtree(scratch, ignore_errors=True)
    print(f"merged {written} new slide(s); kept {skipped} existing slide(s)")

    renderable_after = [entry for entry in files if entry["name"] not in failures]
    remaining = len(pending_fixtures(goldens, renderable_after))
    report_rate(started, rendered_decks, rendered_slides, manifest, remaining)
    if failures:
        print(f"unrenderable decks: {len(failures)} (see {goldens / FAILURES})")

    if full_corpus and references_complete(goldens, renderable_after):
        write_stamp(goldens, reference_stamp(manifest))
        print(f"references complete and stamped: {goldens}")
    else:
        cached = len(renderable_after) - len(pending_fixtures(goldens, renderable_after))
        suffix = f" ({len(failures)} unrenderable)" if failures else ""
        print(
            f"references partial: {cached}/{len(renderable_after)} renderable "
            f"decks cached{suffix}; rerun without --limit to finish"
        )


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
    parser.add_argument(
        "--limit",
        type=positive,
        help="only consider the first N manifest entries (smoke runs)",
    )
    args = parser.parse_args(argv[1:])

    manifest = load_manifest()
    commit = manifest["commit"]
    fixtures = CACHE / "fixtures" / commit
    goldens = CACHE / "goldens" / commit

    print(f"external fixtures: {fixtures}")
    if args.skip_fetch:
        if not fixtures.is_dir():
            raise SystemExit(f"fixtures are missing at {fixtures}")
        print("fetch skipped (--skip-fetch)")
    else:
        command = [
            sys.executable,
            str(FETCH),
            "--manifest",
            str(MANIFEST),
            "--dest",
            str(fixtures),
        ]
        if args.force or args.force_fetch:
            command.append("--force")
        if args.limit is not None:
            command.extend(["--limit", str(args.limit)])
        run(command)

    ensure_viewer(args.offline)

    if args.skip_render:
        print("reference render skipped (--skip-render)")
    elif args.force or args.force_render or needs_render(manifest, goldens, args.limit):
        render_references(
            manifest, fixtures, goldens, args.limit, args.force or args.force_render
        )
    else:
        print(f"references cached: {goldens}")

    if args.skip_test:
        print("comparison skipped (--skip-test)")
        return 0
    if args.limit is not None:
        print(
            "comparison skipped: --limit leaves the reference cache partial; "
            "rerun without --limit to run cargo test"
        )
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
            "--nocapture",
        ],
        cwd=REPO_ROOT,
        env=env,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
