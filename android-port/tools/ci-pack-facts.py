#!/usr/bin/env python3
"""Package only Horizon-computed tables from a full 26.3 preparation.

Minecraft assets/data/pack.png and direct copies of Mojang's worldgen JSON are
intentionally excluded. The Android bootstrap reconstructs those from jars it
downloads directly from Mojang on that device.
"""

from __future__ import annotations

from pathlib import Path
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[2]
VERSION = "26.3"
PREPARED = ROOT / ".local" / "horizon-client" / VERSION
OUTPUT = ROOT / "android-port" / "build" / f"horizon-facts-{VERSION}.zip"

REQUIRED = [
    "horizon-resources.json",
    "facts.json",
    "model-states.json",
    "movement-properties.nbt",
    "attributes.json",
    "landing-definitions.json",
    "damage-types.json",
    "damage-tags.json",
    "damage-catalog.json",
    "player-float-ids.json",
    "cached-collision.nbt",
    "render-definitions.json",
    "occlusion.nbt",
    "gameplay-facts.json",
    "entity-types.json",
    "block-behaviours.json",
    "items.json",
    "registries.json",
    "commands.json",
    "entity-models.json",
    "mob-data.json",
    "command-registries.json",
    "creative-inventory.json",
]

REQUIRED_BLOCKS = [
    "block-properties.nbt",
    "cached-collision.nbt",
    "cached-support.nbt",
    "survival-rules.json",
    "survival-extra-definitions.json",
    "placement-definitions.json",
    "structure-blocks.json",
]


def add_file(z: zipfile.ZipFile, path: Path, arcname: str) -> None:
    if not path.is_file():
        raise RuntimeError(f"prepared resources are missing {arcname}")
    z.write(path, arcname, compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)


def main() -> int:
    if not PREPARED.joinpath("horizon-resources.json").is_file():
        raise RuntimeError(f"no prepared resources at {PREPARED}")
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.unlink(missing_ok=True)
    with zipfile.ZipFile(OUTPUT, "w", allowZip64=True) as z:
        for relative in REQUIRED:
            add_file(z, PREPARED / relative, relative)

        biome_dir = PREPARED / "biome-parameters"
        biomes = sorted(p for p in biome_dir.rglob("*") if p.is_file())
        if not any(p.name == "overworld.json" for p in biomes):
            raise RuntimeError("prepared resources contain no overworld biome parameters")
        for path in biomes:
            z.write(path, path.relative_to(PREPARED).as_posix(), zipfile.ZIP_DEFLATED, 9)

        block_dir = PREPARED / "worldgen" / "blocks"
        for name in REQUIRED_BLOCKS:
            add_file(z, block_dir / name, f"worldgen/blocks/{name}")

        # Some preparations use a Java-exported presets table when data-generator
        # biome reports are unavailable. It is computed data, not a Mojang asset.
        presets = PREPARED / "worldgen" / "presets.json"
        if presets.is_file():
            z.write(presets, "worldgen/presets.json", zipfile.ZIP_DEFLATED, 9)

    print(f"facts: {OUTPUT} ({OUTPUT.stat().st_size / 1024 / 1024:.1f} MiB)")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ci-pack-facts: {exc}", file=sys.stderr)
        raise
