#!/bin/bash

# Verify pre-requisite
if [ "${ANDROID_HOME}" == "" ]; then
  echo "ANDROID_HOME is unset. This should be set in your shell initialization scripts."
  exit 1
fi

# Set the NDK environment variable to the one we need; might vary from system default
cargo install toml-cli
ANDROID_NDK_VERSION=$(toml get gradle/libs.versions.toml versions.ndk --raw)
if [ -z "${ANDROID_NDK_VERSION}" ]; then
  echo "Could not read the NDK version from gradle/libs.versions.toml."
  echo "Run this from the android/backend directory, and make sure toml-cli is installed and on PATH (cargo install toml-cli)."
  echo "Without a version, ANDROID_NDK_HOME would point at the ndk/ parent and the x86_64 link step fails to find libclang_rt.builtins-x86_64-android.a."
  exit 1
fi
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/$ANDROID_NDK_VERSION
if ! [ -e "${ANDROID_NDK_HOME}" ]; then
  echo "Android NDK ${ANDROID_NDK_VERSION} needed for Anki-Android-Backend but not installed."
  echo "Install it with '${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager --install \"ndk;${ANDROID_NDK_VERSION}\"'."
  exit 1
fi

echo "Success with NDK home at ${ANDROID_NDK_HOME}."
