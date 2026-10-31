#!/usr/bin/env python3
"""Build an optimized Rust APK with Android's standard GameActivity adapter.

Requires Android SDK, NDK, Rust Android target, and a JDK (for APK signing).
Generated APKs and the throwaway signing key stay in .local/android.
"""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import zipfile
import json
import hashlib

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {'x86_64': 'x86_64-linux-android', 'arm64-v8a': 'aarch64-linux-android'}


def latest(directory):
    return max(directory.iterdir(), key=lambda p: tuple(int(v) for v in p.name.split('.')))


def run(command, **kwargs):
    print(' '.join(map(str, command)), flush=True)
    subprocess.run(list(map(str, command)), check=True, **kwargs)


def engine_data():
    version = json.loads((ROOT / 'research-lock.json').read_text())['minecraft']['version']
    resources = ROOT / '.local/horizon-client' / version
    if not (resources / 'horizon-resources.json').is_file():
        raise FileNotFoundError('Developer build needs prepared engine tables; run tools/android_prepare.py once')
    files = [p for p in resources.iterdir() if p.is_file() and p.suffix in ('.json', '.nbt') and p.name != 'asset-index.json']
    for name in ['worldgen', 'biome-parameters']:
        files.extend(p for p in (resources / name).rglob('*') if p.is_file())
    files.sort()
    fingerprint = hashlib.sha256(''.join(f'{p.relative_to(resources)}:{p.stat().st_size}:{p.stat().st_mtime_ns}\n' for p in files).encode()).hexdigest()
    output = ROOT / '.local/android/engine-data.zip'
    stamp = output.with_suffix('.fingerprint')
    if not output.exists() or not stamp.exists() or stamp.read_text() != fingerprint:
        output.parent.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(output.with_suffix('.partial'), 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=1) as archive:
            for file in files:
                archive.write(file, file.relative_to(resources).as_posix())
        output.with_suffix('.partial').replace(output)
        stamp.write_text(fingerprint)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--abi', choices=TARGETS, default='arm64-v8a')
    parser.add_argument('--sdk', type=Path, default=os.environ.get('ANDROID_HOME') or Path(os.environ.get('LOCALAPPDATA', str(Path.home()))) / 'Android/Sdk')
    parser.add_argument('--java-home', type=Path, default=os.environ.get('JAVA_HOME') or 'C:/Program Files/Android/Android Studio/jbr')
    parser.add_argument('--skip-rust', action='store_true')
    parser.add_argument('--profile', choices=['fast', 'dev'], default='fast', help='Optimized by default; dev is for debugging only')
    parser.add_argument('--gradle', type=Path, help='Gradle 8.14 executable (otherwise discover the cached distribution)')
    parser.add_argument('--check', action='store_true', help='Only type-check the Android target')
    parser.add_argument('--clippy', action='store_true', help='Run Android Clippy with warnings denied')
    parser.add_argument('--bootstrap', type=Path, help=argparse.SUPPRESS)  # Old scripts: no longer used.
    args = parser.parse_args()
    suffix = '.exe' if os.name == 'nt' else ''
    tools = latest(args.sdk / 'build-tools')
    ndk = latest(args.sdk / 'ndk') / 'toolchains/llvm/prebuilt' / ('windows-x86_64' if os.name == 'nt' else 'linux-x86_64')
    target = TARGETS[args.abi]
    environment = dict(os.environ)
    environment['PATH'] = os.pathsep.join([str(ndk / 'bin'), str(args.java_home / 'bin'), environment['PATH']])
    environment['JAVA_HOME'] = str(args.java_home)
    triple = target.replace('-', '_')
    clang = ndk / 'bin' / (target + '28-clang' + ('.cmd' if os.name == 'nt' else ''))
    environment[f'CC_{triple}'] = str(clang)
    environment[f'CXX_{triple}'] = str(ndk / 'bin' / (target + '28-clang++' + ('.cmd' if os.name == 'nt' else '')))
    environment[f'AR_{triple}'] = str(ndk / 'bin' / ('llvm-ar' + suffix))
    environment[f'CARGO_TARGET_{triple.upper()}_LINKER'] = str(clang)
    environment['CARGO_PROFILE_DEV_DEBUG'] = '0'
    if args.check or args.clippy:
        command = ['cargo', 'clippy' if args.clippy else 'check', '-p', 'horizon-android', '--target', target]
        if args.clippy:
            command += ['--no-deps']
            command += ['--', '-D', 'warnings']
        run(command, cwd=ROOT, env=environment)
        return
    if not args.skip_rust:
        run(['cargo', 'build', '-p', 'horizon-android', '--target', target, '--profile', args.profile], cwd=ROOT, env=environment)
    output = ROOT / '.local/android/apk' / args.abi
    output.mkdir(parents=True, exist_ok=True)
    library = ROOT / 'target' / target / ('debug' if args.profile == 'dev' else args.profile) / 'libhorizon_android.so'
    if not library.is_file():
        raise FileNotFoundError(library)
    staging = ROOT / '.local/android/gradle-jni' / args.abi
    staging.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(library, staging / library.name)
    run([ndk / 'bin' / ('llvm-strip' + suffix), '--strip-debug', staging / library.name])
    assets = ROOT / '.local/android/gradle-assets'
    assets.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(engine_data(), assets / 'engine-data.zip')
    controls = ROOT / '.local/android/bedrock-controls.zip'
    if not controls.is_file():
        import sys
        run([sys.executable, ROOT / 'tools/android_bedrock_controls.py'], cwd=ROOT)
    shutil.copyfile(controls, assets / 'bedrock-controls.zip')
    key = ROOT / '.local/android/debug.keystore'
    if not key.exists():
        run([args.java_home / 'bin' / ('keytool' + suffix), '-genkeypair', '-keystore', key, '-storepass', 'android', '-keypass', 'android', '-alias', 'androiddebugkey', '-keyalg', 'RSA', '-validity', '3650', '-dname', 'CN=Horizon Android Development'])
    gradle = args.gradle
    if gradle is None:
        candidates = list((Path.home() / '.gradle/wrapper/dists/gradle-8.14-all').glob('*/gradle-8.14/bin/gradle' + ('.bat' if os.name == 'nt' else '')))
        if not candidates:
            raise FileNotFoundError('Install Gradle 8.14 or pass --gradle PATH')
        gradle = candidates[0]
    environment['ANDROID_HOME'] = str(args.sdk)
    run([gradle, '--no-daemon', '-p', ROOT / 'packaging/android', '-Pabi=' + args.abi, 'assembleDebug'], cwd=ROOT, env=environment)
    apk = output / 'horizon-android.apk'
    shutil.copyfile(ROOT / '.local/android/gradle-build/outputs/apk/debug/HorizonAndroid-debug.apk', apk)
    # Invoke the signing JAR directly so Windows does not need a batch-file shell.
    signer = [args.java_home / 'bin' / ('java' + suffix), '-jar', tools / 'lib/apksigner.jar']
    run([*signer, 'verify', '--verbose', apk])
    print(f'APK: {apk}', flush=True)


if __name__ == '__main__':
    main()
