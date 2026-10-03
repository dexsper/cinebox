#!/usr/bin/env bash
# Build the Android TV APK: the Rust app through cargo-ndk, then Gradle packaging.
#
#   ./scripts/android-build.sh            # release APK (signed when ANDROID_KEYSTORE_PATH is set)
#   ./scripts/android-build.sh debug      # debug-signed APK for adb install
#
# Needs: Android SDK (ANDROID_HOME) with an NDK, JDK 17+, the Rust targets
# aarch64-linux-android and armv7-linux-androideabi, and cargo-ndk:
#   cargo install cargo-ndk --locked
#
# The Rust library is always built --release: a debug build is too slow to play video.

set -euo pipefail

variant="${1:-release}"
case "$variant" in
  release) task=assembleRelease ;;
  debug) task=assembleDebug ;;
  *)
    echo "usage: $0 [release|debug]" >&2
    exit 1
    ;;
esac

if ! command -v cargo-ndk >/dev/null 2>&1; then
  echo "cargo-ndk is not on PATH. Install: cargo install cargo-ndk --locked" >&2
  exit 1
fi

repo="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo"

# minSdk in android/app/build.gradle.kts.
cargo ndk \
  -t arm64-v8a -t armeabi-v7a \
  --platform 26 \
  build -p cinebox-android --release --locked

# Not cargo-ndk's -o: it also copies host cdylibs that build scripts depend on
# (sevenz-rust2), which would ship x86-64 libraries inside the ARM folders.
jni_libs=android/app/src/main/jniLibs
rm -rf "$jni_libs"
for abi_triple in arm64-v8a:aarch64-linux-android armeabi-v7a:armv7-linux-androideabi; do
  abi="${abi_triple%%:*}"
  triple="${abi_triple#*:}"
  mkdir -p "$jni_libs/$abi"
  cp "target/$triple/release/libcinebox_android.so" "$jni_libs/$abi/"
done

cd android
./gradlew --no-daemon "$task"

find "$repo/android/app/build/outputs/apk" -name '*.apk' -print
