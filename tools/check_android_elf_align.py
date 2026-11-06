#!/usr/bin/env python3
"""Verify every ARM64 LOAD segment in Horizon's packaged APK is 16 KiB aligned.

Reads ZIP + ELF program headers using the Python standard library, so CI
doesn't depend on an NDK-specific llvm-readelf installation.
"""
from __future__ import annotations
import argparse
import struct
import zipfile
from pathlib import Path

PT_LOAD = 1
PAGE = 16384

def check_elf(data: bytes, name: str) -> list[int]:
    if data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        raise ValueError(f"{name}: expected little-endian ELF64")
    if struct.unpack_from("<H", data, 18)[0] != 183:
        raise ValueError(f"{name}: expected AArch64 machine 183")
    phoff = struct.unpack_from("<Q", data, 32)[0]
    phentsize = struct.unpack_from("<H", data, 54)[0]
    phnum = struct.unpack_from("<H", data, 56)[0]
    if not phnum or phentsize < 56 or phoff + phentsize * phnum > len(data):
        raise ValueError(f"{name}: invalid program headers")
    alignments = []
    for i in range(phnum):
        off = phoff + phentsize * i
        if struct.unpack_from("<I", data, off)[0] == PT_LOAD:
            alignment = struct.unpack_from("<Q", data, off + 48)[0]
            if alignment < PAGE:
                raise ValueError(f"{name}: PT_LOAD[{i}] align {alignment} < {PAGE}")
            alignments.append(alignment)
    if not alignments:
        raise ValueError(f"{name}: no PT_LOAD segments")
    print(f"ELF_16K_OK {name}: PT_LOAD alignments={alignments}")
    return alignments

def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("apk", type=Path)
    args = parser.parse_args()
    with zipfile.ZipFile(args.apk) as z:
        names = [n for n in z.namelist() if n.startswith("lib/arm64-v8a/") and n.endswith(".so")]
        if not names:
            parser.error("No packaged ARM64 libraries")
        for name in names:
            check_elf(z.read(name), name)

if __name__ == "__main__":
    main()
