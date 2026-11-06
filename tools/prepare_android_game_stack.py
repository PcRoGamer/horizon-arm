#!/usr/bin/env python3
"""Patch Android GameActivity's C game-loop stack without forking Cargo crates.

android-activity 0.6.1 uses an upstream C pthread entrypoint for GameActivity.
Unlike Rust-spawned threads, that entrypoint does not honor RUST_MIN_STACK.
Horizon builds a large real-world renderer at the loading handoff, so the
Android 1 MiB pthread default can overflow before Loading::finish runs.

The crate's build.rs explicitly supports ANDROID_GAMES_SDK to replace the
upstream SDK source directory. Keep all its headers and code unchanged except
for the one native thread's pthread_attr_setstacksize call. No patching the
Cargo registry, vendoring the SDK, or altering other threads.
"""
from __future__ import annotations

import os
import shutil
import sys
from pathlib import Path

VERSION = "0.6.1"
GAME_MAIN_STACK_BYTES = 16 * 1024 * 1024
GLUE = Path(
    "game-activity/prefab-src/modules/game-activity/"
    "src/game-activity/native_app_glue/android_native_app_glue.c"
)
BEFORE = (
    "    pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);\n"
    "    pthread_create(&android_app->thread, &attr, android_app_entry, android_app);"
)
AFTER = (
    "    pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);\n"
    "    // The game handoff constructs large Rust render state on android_main.\n"
    "    // This pthread is created by C glue, so RUST_MIN_STACK cannot resize it.\n"
    "    if (pthread_attr_setstacksize(&attr, 16 * 1024 * 1024) != 0) {\n"
    '        LOGE("Failed to set 16 MiB Android game-loop stack");\n'
    "    }\n"
    "    pthread_create(&android_app->thread, &attr, android_app_entry, android_app);"
)


def prepare(root: Path, output: Path) -> Path:
    versions = list(root.glob(f"registry/src/*/android-activity-{VERSION}/android-games-sdk"))
    if len(versions) != 1:
        raise RuntimeError(
            f"Expected exactly one fetched android-activity {VERSION} crate in {root}; "
            f"got {len(versions)}. Run cargo fetch --locked first."
        )
    source = versions[0]
    if not (source / GLUE).is_file():
        raise FileNotFoundError(source / GLUE)
    output = output.resolve()
    if output == source.resolve():
        raise ValueError("Never patch the read-only Cargo registry source")
    if not output.is_dir():
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(source, output)
    glue = output / GLUE
    original = glue.read_text(encoding="utf-8")
    if original.count(AFTER) == 1:
        pass
    elif original.count(BEFORE) == 1:
        glue.write_text(original.replace(BEFORE, AFTER), encoding="utf-8")
    else:
        raise RuntimeError("Unknown upstream glue; refusing to patch unverified C code")
    if glue.read_text(encoding="utf-8").count(AFTER) != 1:
        raise AssertionError("Game-loop stack override was not applied")
    return output


def main() -> int:
    repo = Path(__file__).resolve().parents[1]
    cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")))
    destination = repo / ".local" / "android" / f"game-activity-{VERSION}-stack"
    result = prepare(cargo_home, destination)
    print(f"Android GameActivity C glue: 16 MiB stack via {result}", file=sys.stderr)
    print(result)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
