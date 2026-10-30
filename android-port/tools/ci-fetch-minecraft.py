#!/usr/bin/env python3
"""Fetch the pinned Minecraft inputs used only to compute Horizon facts in CI.

Nothing fetched by this script is shipped in the APK. The resulting fact pack is
filtered separately to Horizon-computed tables; phones fetch their own jars from
Mojang on first launch.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import sys
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
LOCK = json.loads((ROOT / "research-lock.json").read_text())['minecraft']
VERSION = LOCK['version']
OUT = ROOT / "android-port" / "build" / "ci-minecraft"
FETCH_ATTEMPTS = 4


def sha1(path: Path) -> str:
    h = hashlib.sha1()
    with path.open('rb') as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def safe(relative: str) -> Path:
    p = PurePosixPath(relative)
    if p.is_absolute() or any(part in ('', '.', '..') for part in p.parts):
        raise RuntimeError(f"unsafe Mojang path: {relative!r}")
    return Path(*p.parts)


def fetch(url: str, path: Path, expected: str | None = None, size: int | None = None) -> None:
    if path.is_file() and (size is None or path.stat().st_size == size):
        if expected is None or sha1(path) == expected:
            return
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_suffix(path.suffix + '.part')
    req = urllib.request.Request(url, headers={'User-Agent': 'horizon-android-ci/0.1'})

    last_error: Exception | None = None
    for attempt in range(1, FETCH_ATTEMPTS + 1):
        temp.unlink(missing_ok=True)
        suffix = '' if attempt == 1 else f' (attempt {attempt}/{FETCH_ATTEMPTS})'
        print(f"fetch {url}{suffix}", flush=True)
        try:
            with urllib.request.urlopen(req, timeout=120) as response, temp.open('wb') as out:
                shutil.copyfileobj(response, out, 1024 * 1024)
            if size is not None and temp.stat().st_size != size:
                raise RuntimeError(
                    f"{url}: wrong size {temp.stat().st_size}, expected {size}"
                )
            if expected is not None:
                actual = sha1(temp)
                if actual != expected:
                    raise RuntimeError(f"{url}: sha1 {actual}, expected {expected}")
            temp.replace(path)
            return
        except (OSError, RuntimeError, urllib.error.URLError) as exc:
            last_error = exc
            temp.unlink(missing_ok=True)
            if attempt == FETCH_ATTEMPTS:
                break
            delay = min(2 ** attempt, 10)
            print(f"retrying in {delay}s after: {exc}", file=sys.stderr, flush=True)
            time.sleep(delay)

    assert last_error is not None
    raise RuntimeError(
        f"{url}: failed after {FETCH_ATTEMPTS} attempts: {last_error}"
    ) from last_error


def main() -> int:
    manifest_sha1 = LOCK['manifest_sha1']
    manifest_url = (
        f"https://piston-meta.mojang.com/v1/packages/{manifest_sha1}/{VERSION}.json"
    )
    version_dir = OUT / 'versions' / VERSION
    manifest_path = version_dir / f'{VERSION}.json'
    fetch(manifest_url, manifest_path, manifest_sha1)
    manifest = json.loads(manifest_path.read_text())
    if manifest.get('id') != VERSION:
        raise RuntimeError('pinned Mojang manifest is for the wrong version')

    for side in ('client', 'server'):
        item = manifest['downloads'][side]
        pin = LOCK[f'{side}_sha1']
        if item['sha1'] != pin:
            raise RuntimeError(f'{side} pin disagrees with Mojang manifest')
        target = (
            version_dir / f'{VERSION}.jar'
            if side == 'client'
            else OUT / 'server.jar'
        )
        fetch(item['url'], target, item['sha1'], item.get('size'))

    # The Java exporters need their client classpath. Download every ordinary
    # artifact listed by Mojang; horizon-prepare itself applies the platform
    # rules and selects the Linux/x86-64 subset it needs.
    seen: set[str] = set()
    artifacts = []
    for library in manifest.get('libraries', []):
        artifact = library.get('downloads', {}).get('artifact')
        if not artifact or not artifact.get('path') or not artifact.get('url'):
            continue
        relative = artifact['path']
        if relative in seen:
            continue
        seen.add(relative)
        artifacts.append(artifact)

    print(f"fetching {len(artifacts)} client libraries", flush=True)
    for artifact in artifacts:
        fetch(
            artifact['url'],
            OUT / 'libraries' / safe(artifact['path']),
            artifact.get('sha1'),
            artifact.get('size'),
        )

    print(f"Minecraft {VERSION} CI inputs ready at {OUT}", flush=True)
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ci-fetch-minecraft: {exc}", file=sys.stderr)
        raise
