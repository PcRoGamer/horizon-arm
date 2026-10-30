#!/usr/bin/env bash
# The environment every Android build here uses. Source it: `. tools/env.sh`.
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TC="$REPO/.toolchain"
# android-port/.toolchain is a tracked relative symlink to a shared cache.
# Resolve it before exporting SDK/NDK/compiler paths; otherwise disposable
# upstream trees point cc-rs/ring at a non-existent path through the symlink.
if [ -L "$TC" ]; then
  TC_LINK="$(readlink "$TC")"
  case "$TC_LINK" in
    /*) TC="$TC_LINK" ;;
    *) TC="$(dirname "$TC")/$TC_LINK" ;;
  esac
fi
export ANDROID_HOME="$TC/android-sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.2.12479018"
export JAVA_HOME="$TC/jdk"
export PATH="$HOME/.cargo/bin:$JAVA_HOME/bin:$ANDROID_HOME/emulator:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/build-tools/34.0.0:$PATH"
# The game itself: this checkout's root, one level above android-port/.
export HORIZON_SRC="${HORIZON_SRC:-$(cd "$REPO/.." && pwd)}"
export HORIZON_BUILD="$REPO/build"
[ -d "$HORIZON_SRC" ] || { echo "env.sh: no horizon checkout at $HORIZON_SRC" >&2; return 1 2>/dev/null || exit 1; }

# NDK linkers and the C compiler the crates with C code (ring, and build scripts) use.
ndk_cc() { echo "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/$1"; }
export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER
CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$(ndk_cc x86_64-linux-android26-clang)"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER
CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$(ndk_cc aarch64-linux-android26-clang)"
export CC_x86_64_linux_android
CC_x86_64_linux_android="$(ndk_cc x86_64-linux-android26-clang)"
export CC_aarch64_linux_android
CC_aarch64_linux_android="$(ndk_cc aarch64-linux-android26-clang)"
export AR_x86_64_linux_android
AR_x86_64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
export AR_aarch64_linux_android
AR_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
