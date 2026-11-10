#!/bin/sh
# Checks BananaV for macOS from Windows (Git Bash) or Linux: clippy for the
# Mac's targets, with nothing of macOS installed here — the same way as
# tools/linux_check.sh, which it runs with a Mac target (FFmpeg's
# declarations from the CI stand in for its headers; without them
# `--cfg bv_no_ffmpeg`).
#
#   tools/mac_check.sh [-p crate …] [cargo clippy arguments]
#   (default: the whole workspace, all targets, warnings as errors)
#
# Targets: aarch64-apple-darwin (Apple's chips) by default; MAC_TARGET
# changes it (x86_64-apple-darwin, the Intel Macs). Needs `rustup target
# add` of it. Nothing is linked: the Mac's linker and libraries are only on
# a Mac (GitHub's, through `.github/workflows/ci.yml`'s macOS job).

set -e
root=$(cd "$(dirname "$0")/.." && pwd)
LINUX_TARGET=${MAC_TARGET:-aarch64-apple-darwin} exec sh "$root/tools/linux_check.sh" "$@"
