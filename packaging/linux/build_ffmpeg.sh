#!/bin/sh
# Builds the FFmpeg of BananaV's Linux packages: shared libraries under the
# LGPL only (FFmpeg's GPL parts, x264 and x265 left out), with the
# codec libraries the system has — dav1d, libvpx, libaom, SVT-AV1,
# OpenH264, kvazaar, Opus, LAME — and the graphics cards' VA-API and
# NVIDIA support (nv-codec-headers: FFmpeg loads NVIDIA's libraries at run
# time).
#
#   packaging/linux/build_ffmpeg.sh <prefix>
#
# Needs a C compiler, nasm, pkg-config, git and the libraries' development
# packages (see .github/workflows/package.yml). FFMPEG_VERSION and
# NV_CODEC_HEADERS change the versions.

set -eu
prefix=$1
ffmpeg_version=${FFMPEG_VERSION:-7.1.1}
nv_headers=${NV_CODEC_HEADERS:-n12.1.14.0}
work=$(mktemp -d)
mkdir -p "$prefix"

git clone -q --depth 1 --branch "$nv_headers" https://github.com/FFmpeg/nv-codec-headers "$work/nv"
make -C "$work/nv" install PREFIX="$prefix"
export PKG_CONFIG_PATH="$prefix/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"

git clone -q --depth 1 --branch "n$ffmpeg_version" https://github.com/FFmpeg/FFmpeg "$work/ffmpeg"

# The codec libraries this system has (pkg-config name : FFmpeg's switch).
libs=""
for pair in dav1d:libdav1d vpx:libvpx aom:libaom SvtAv1Enc:libsvtav1 \
    openh264:libopenh264 kvazaar:libkvazaar opus:libopus; do
    if pkg-config --exists "${pair%%:*}"; then
        libs="$libs --enable-${pair#*:}"
    fi
done
# LAME has no pkg-config file.
if [ -f /usr/include/lame/lame.h ]; then
    libs="$libs --enable-libmp3lame"
fi
echo "FFmpeg $ffmpeg_version with:$libs"

cd "$work/ffmpeg"
# shellcheck disable=SC2086
./configure --prefix="$prefix" \
    --enable-shared --disable-static --enable-pic \
    --disable-programs --disable-doc --disable-network \
    --disable-avdevice --disable-avfilter --disable-postproc \
    --disable-autodetect --enable-pthreads --enable-zlib --enable-bzlib --enable-lzma \
    --enable-vaapi --enable-libdrm \
    --enable-ffnvcodec --enable-nvdec --enable-nvenc \
    $libs
make -j"$(nproc)"
make install
# The LGPL's text and FFmpeg's notice go with the libraries.
mkdir -p "$prefix/share/licenses/ffmpeg"
cp COPYING.LGPLv2.1 LICENSE.md "$prefix/share/licenses/ffmpeg/"
rm -rf "$work"
