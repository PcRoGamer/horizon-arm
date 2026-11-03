#!/usr/bin/env python3
"""Build the small Horizon-generated engine-data bundle used by the Android APK.

Minecraft-owned inputs are downloaded directly from the official Mojang URLs in
its version metadata and verified before `horizon-prepare` sees them. The APK
receives only Horizon's generated JSON/NBT/worldgen tables; the client JAR,
textures, sounds and asset objects remain outside the APK and are downloaded
again on-device from Mojang by `crates/horizon-android/src/bootstrap.rs`.
"""
from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import tempfile
import time
import urllib.parse
import urllib.request
import zipfile

import lab

ROOT = Path(__file__).resolve().parents[1]
LOCK = json.loads((ROOT / "research-lock.json").read_text(encoding="utf-8"))
GAME = LOCK["minecraft"]
VERSION = GAME["version"]
CATALOG = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"
ALLOWED = {
    "piston-meta.mojang.com",
    "piston-data.mojang.com",
    "resources.download.minecraft.net",
    "launcher.mojang.com",
    "launchermeta.mojang.com",
    "libraries.minecraft.net",
}


def official(url: str) -> str:
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme != "https" or parsed.hostname not in ALLOWED or parsed.username or parsed.password:
        raise ValueError(f"Not an official Mojang HTTPS URL: {url}")
    return url


def fetch_json(url: str):
    request = urllib.request.Request(official(url), headers={"User-Agent": "Horizon-Android/0.1"})
    with urllib.request.urlopen(request, timeout=120) as response:
        if response.geturl() != url:
            raise ValueError(f"Unexpected redirect while fetching {url}")
        return json.load(response)


def safe(base: Path, relative: str) -> Path:
    p = PurePosixPath(relative)
    if p.is_absolute() or ".." in p.parts or "\\" in relative:
        raise ValueError(f"Unsafe Mojang path: {relative}")
    return base.joinpath(*p.parts)


def download(url: str, destination: Path, sha1: str):
    official(url)
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.is_file() and lab.digest(destination, "sha1") == sha1:
        return destination
    for attempt in range(4):
        temp = None
        try:
            with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as out:
                temp = Path(out.name)
                request = urllib.request.Request(url, headers={"User-Agent": "Horizon-Android/0.1"})
                with urllib.request.urlopen(request, timeout=120) as response:
                    if response.geturl() != url:
                        raise ValueError(f"Unexpected redirect while fetching {url}")
                    shutil.copyfileobj(response, out)
            actual = lab.digest(temp, "sha1")
            if actual != sha1:
                raise ValueError(f"SHA-1 mismatch for {destination.name}: {actual} != {sha1}")
            temp.replace(destination)
            return destination
        except Exception:
            if temp is not None:
                temp.unlink(missing_ok=True)
            if attempt == 3:
                raise
            time.sleep(attempt + 1)


def parallel(files):
    with ThreadPoolExecutor(max_workers=8) as pool:
        list(pool.map(lambda item: download(*item), files))


def prepare_launcher_tree() -> tuple[Path, Path]:
    base = ROOT / ".local/android/mojang"
    catalog = fetch_json(CATALOG)
    entry = next((v for v in catalog["versions"] if v["id"] == VERSION), None)
    if entry is None:
        raise ValueError(f"Mojang does not list pinned Minecraft {VERSION}")
    accepted = {GAME["manifest_sha1"], *GAME.get("accepted_manifest_sha1", [])}
    if entry["sha1"] not in accepted:
        raise ValueError(
            f"Mojang changed {VERSION} metadata ({entry['sha1']}); update research-lock.json first"
        )

    version_dir = base / "versions" / VERSION
    manifest_path = version_dir / f"{VERSION}.json"
    download(entry["url"], manifest_path, entry["sha1"])
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest["id"] != VERSION:
        raise ValueError("Downloaded version metadata describes another release")

    files: list[tuple[str, Path, str]] = []
    client = manifest["downloads"]["client"]
    server = manifest["downloads"]["server"]
    if client["sha1"] != GAME["client_sha1"] or server["sha1"] != GAME["server_sha1"]:
        raise ValueError("Mojang client/server does not match Horizon's research lock")
    files.append((client["url"], version_dir / f"{VERSION}.jar", client["sha1"]))
    server_path = base / "server" / f"{VERSION}.jar"
    files.append((server["url"], server_path, server["sha1"]))

    for artifact in lab.client_library_artifacts(manifest):
        files.append((artifact["url"], safe(base / "libraries", artifact["path"]), artifact["sha1"]))

    index = manifest["assetIndex"]
    index_path = base / "assets" / "indexes" / f"{index['id']}.json"
    download(index["url"], index_path, index["sha1"])
    objects = json.loads(index_path.read_text(encoding="utf-8"))["objects"]
    for value in objects.values():
        checksum = value["hash"]
        relative = f"{checksum[:2]}/{checksum}"
        files.append(
            (
                f"https://resources.download.minecraft.net/{relative}",
                safe(base / "assets" / "objects", relative),
                checksum,
            )
        )

    print(f"Verified Mojang inputs: {len(files)} files", flush=True)
    parallel(files)
    return base, server_path


def package_engine_data() -> Path:
    resources = ROOT / ".local/horizon-client" / VERSION
    marker = resources / "horizon-resources.json"
    if not marker.is_file():
        raise FileNotFoundError(marker)

    files: list[Path] = []
    files.extend(
        p
        for p in resources.iterdir()
        if p.is_file() and p.suffix in {".json", ".nbt"} and p.name != "asset-index.json"
    )
    for directory in ("worldgen", "biome-parameters"):
        root = resources / directory
        if root.is_dir():
            files.extend(p for p in root.rglob("*") if p.is_file())
    files = sorted(set(files))
    if marker not in files:
        raise RuntimeError("engine-data selection omitted horizon-resources.json")

    output = ROOT / ".local/android/engine-data.zip"
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_suffix(".partial")
    with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=1) as archive:
        for file in files:
            archive.write(file, file.relative_to(resources).as_posix())
    temporary.replace(output)
    print(
        f"Horizon engine data: {output} ({output.stat().st_size / 1024 / 1024:.1f} MiB, {len(files)} files)",
        flush=True,
    )
    return output


def main():
    minecraft, server = prepare_launcher_tree()
    env = dict(os.environ)
    env["HORIZON_NO_AUDIO"] = "1"
    env["HORIZON_LAUNCHER_NO_SELF_UPDATE"] = "1"
    subprocess.run(
        [
            "cargo",
            "run",
            "-p",
            "horizon-prepare",
            "--",
            "--root",
            str(ROOT),
            "--prepare",
            "--minecraft-dir",
            str(minecraft),
            "--server-jar",
            str(server),
        ],
        cwd=ROOT,
        env=env,
        check=True,
    )
    package_engine_data()


if __name__ == "__main__":
    main()
