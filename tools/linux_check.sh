#!/bin/sh
# Checks BananaV for Linux from Windows (Git Bash): clippy for the Linux
# target, with nothing of Linux installed here.
#
#   tools/linux_check.sh [-p crate …] [cargo clippy arguments]
#   (default: the whole workspace, all targets, warnings as errors)
#
# - ALSA's pkg-config answer is faked (only the bindings' build asks it).
# - FFmpeg: its headers are not here, so its Rust declarations come from the
#   CI (artifact "ffmpeg-bindings" of the last run of this branch, fetched
#   with gh into target/linux-check), wrapped as a stand-in ffmpeg-sys-next
#   patched in for this check only (Cargo.lock is put back afterwards).
#   Without them, `--cfg bv_no_ffmpeg` checks the rest without FFmpeg.
# - Builds go to target/linux-check.
#
# Targets: x86_64-unknown-linux-gnu by default; LINUX_TARGET changes it
# (aarch64-unknown-linux-gnu). Needs `rustup target add` of it.

set -e
root=$(cd "$(dirname "$0")/.." && pwd)
work="$root/target/linux-check"
target=${LINUX_TARGET:-x86_64-unknown-linux-gnu}
mkdir -p "$work"
cd "$root"

# pkg-config: a version for --modversion, ALSA's library otherwise.
cat > "$work/pkg-config.cmd" <<'EOF'
@echo off
echo %* | findstr /C:"--modversion" >nul && (echo 1.2.11& exit /b 0)
echo -lasound
exit /b 0
EOF

# FFmpeg's bindings from the CI, once: the newest run of this branch (or of
# main) that kept them.
bindings="$work/ffmpeg-bindings.rs"
if [ ! -f "$bindings" ] && command -v gh >/dev/null 2>&1; then
    for branch in "$(git rev-parse --abbrev-ref HEAD)" main; do
        for run in $(gh run list --branch "$branch" --workflow ci.yml --limit 20 \
            --json databaseId --jq '.[].databaseId' 2>/dev/null); do
            rm -rf "$work/artifact"
            if gh run download "$run" -n ffmpeg-bindings -D "$work/artifact" >/dev/null 2>&1; then
                found=$(find "$work/artifact" -name bindings.rs | head -1)
                if [ -n "$found" ]; then
                    cp "$found" "$bindings"
                    break 2
                fi
            fi
        done
    done
fi

patch=""
flags=""
if [ -f "$bindings" ]; then
    # The stand-in: ffmpeg-sys-next's own sources around the CI's bindings.
    real=$(ls -d "$HOME"/.cargo/registry/src/*/ffmpeg-sys-next-9.* 2>/dev/null | tail -1)
    if [ -z "$real" ]; then
        cargo fetch -q
        real=$(ls -d "$HOME"/.cargo/registry/src/*/ffmpeg-sys-next-9.* | tail -1)
    fi
    stand="$work/ffmpeg-sys-next"
    rm -rf "$stand"
    mkdir -p "$stand/src"
    cp -r "$real/src/avutil" "$stand/src/"
    cp "$bindings" "$stand/bindings.rs"
    sed 's#include!(concat!(env!("OUT_DIR"), "/bindings.rs"));#include!("../bindings.rs");#' \
        "$real/src/lib.rs" > "$stand/src/lib.rs"
    version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$real/Cargo.toml" | head -1)
    cat > "$stand/Cargo.toml" <<EOF
[package]
name = "ffmpeg-sys-next"
version = "$version"
edition = "2024"
links = "ffmpeg"
build = "build.rs"

[dependencies]
libc = "0.2"

[features]
default = ["avcodec", "avdevice", "avfilter", "avformat", "swresample", "swscale"]
avcodec = []
avdevice = ["avformat"]
avfilter = []
avformat = ["avcodec"]
swresample = []
swscale = []
ffmpeg_8_0 = []
EOF
    echo 'fn main() {}' > "$stand/build.rs"
    patch="patch.crates-io.ffmpeg-sys-next.path='$(cygpath -m "$stand")'"
else
    echo "linux_check: no FFmpeg bindings from the CI; checking without FFmpeg" >&2
    flags="--cfg bv_no_ffmpeg"
fi

if [ $# -eq 0 ]; then
    set -- --workspace --all-targets -- -D warnings
fi

cp Cargo.lock "$work/Cargo.lock.saved"
status=0
PKG_CONFIG_ALLOW_CROSS=1 PKG_CONFIG="$(cygpath -w "$work/pkg-config.cmd")" \
    RUSTFLAGS="$flags" \
    cargo clippy --target "$target" --target-dir "$work" \
    ${patch:+--config "$patch"} "$@" || status=$?
cp "$work/Cargo.lock.saved" Cargo.lock
exit $status
