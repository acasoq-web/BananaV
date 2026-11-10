#!/bin/sh
# Builds the FFmpeg of BananaV's macOS package for this Mac's processor
# (Apple's chips or Intel): shared libraries under the LGPL only (FFmpeg's
# GPL parts left out), with the Mac's own VideoToolbox (H.264 and HEVC on
# the media engine, decoding and encoding) and AudioToolbox, and three
# libraries built here too — dav1d (AV1 decoding), LAME (MP3 encoding,
# LGPL) and SVT-AV1 (AV1 encoding) —, for macOS 11 and later.
#
#   packaging/macos/build_ffmpeg.sh <prefix>
#
# Needs Xcode's command-line tools and nasm, meson, ninja, cmake and
# pkg-config (Homebrew: `brew install nasm meson ninja cmake pkgconf`).
# FFMPEG_VERSION, DAV1D_VERSION, LAME_VERSION and SVTAV1_VERSION change the
# versions; MACOSX_DEPLOYMENT_TARGET the oldest macOS (11.0).
#
# The libraries' licenses go to <prefix>/share/licenses/<name>/; their
# sources are the tags and the tarball named here (the LGPL asks for
# FFmpeg's and LAME's to go with the package: package.yml publishes them).

set -eu
prefix=$1
ffmpeg_version=${FFMPEG_VERSION:-7.1.1}
dav1d_version=${DAV1D_VERSION:-1.5.1}
lame_version=${LAME_VERSION:-3.100}
svtav1_version=${SVTAV1_VERSION:-v2.3.0}
export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-11.0}
work=$(mktemp -d)
jobs=$(sysctl -n hw.ncpu)
mkdir -p "$prefix/share/licenses"
export PKG_CONFIG_PATH="$prefix/lib/pkgconfig"
min="-mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET"

# dav1d: AV1 decoding on the processor (Macs without AV1 in their media
# engine).
git clone -q --depth 1 --branch "$dav1d_version" https://code.videolan.org/videolan/dav1d.git "$work/dav1d"
meson setup "$work/dav1d/build" "$work/dav1d" --prefix="$prefix" --libdir=lib \
    --buildtype=release --default-library=shared -Denable_tools=false -Denable_tests=false
ninja -C "$work/dav1d/build" install
mkdir -p "$prefix/share/licenses/dav1d"
cp "$work/dav1d/COPYING" "$prefix/share/licenses/dav1d/"

# LAME: MP3 encoding. Its symbol list names functions the library does
# not have here — one its sources no longer have, and the decoder's
# (`hip_…`, `lame_decode…`), left out with the decoder —, which Apple's
# linker refuses: they leave the list too.
curl -fsSL --retry 5 -o "$work/lame.tar.gz" \
    "https://downloads.sourceforge.net/project/lame/lame/$lame_version/lame-$lame_version.tar.gz"
tar -xzf "$work/lame.tar.gz" -C "$work"
(
    cd "$work/lame-$lame_version"
    sed -i '' -e '/lame_init_old/d' -e '/^hip_/d' -e '/^lame_decode/d' include/libmp3lame.sym
    CFLAGS="-O2 $min" LDFLAGS="$min" ./configure --prefix="$prefix" \
        --enable-shared --disable-static --disable-frontend --disable-decoder
    make -j"$jobs"
    make install
)
mkdir -p "$prefix/share/licenses/lame"
cp "$work/lame-$lame_version/COPYING" "$prefix/share/licenses/lame/"

# SVT-AV1: AV1 encoding on the processor.
git clone -q --depth 1 --branch "$svtav1_version" https://gitlab.com/AOMediaCodec/SVT-AV1.git "$work/svt"
cmake -S "$work/svt" -B "$work/svt/build" -G Ninja -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$prefix" -DCMAKE_INSTALL_LIBDIR=lib -DCMAKE_INSTALL_NAME_DIR="$prefix/lib" \
    -DBUILD_SHARED_LIBS=ON -DBUILD_APPS=OFF -DBUILD_TESTING=OFF \
    -DCMAKE_OSX_DEPLOYMENT_TARGET="$MACOSX_DEPLOYMENT_TARGET"
ninja -C "$work/svt/build" install
mkdir -p "$prefix/share/licenses/svt-av1"
cp "$work/svt/LICENSE.md" "$work/svt/PATENTS.md" "$prefix/share/licenses/svt-av1/"

git clone -q --depth 1 --branch "n$ffmpeg_version" https://github.com/FFmpeg/FFmpeg "$work/ffmpeg"
cd "$work/ffmpeg"
./configure --prefix="$prefix" \
    --enable-shared --disable-static --enable-pic \
    --disable-programs --disable-doc --disable-network \
    --disable-avdevice --disable-avfilter --disable-postproc \
    --disable-autodetect --enable-pthreads --enable-zlib --enable-bzlib \
    --enable-videotoolbox --enable-audiotoolbox \
    --enable-libdav1d --enable-libmp3lame --enable-libsvtav1 \
    --extra-cflags="$min -I$prefix/include" \
    --extra-ldflags="$min -L$prefix/lib"
make -j"$jobs"
make install
# The LGPL's text and FFmpeg's notice go with the libraries.
mkdir -p "$prefix/share/licenses/ffmpeg"
cp COPYING.LGPLv2.1 LICENSE.md "$prefix/share/licenses/ffmpeg/"
cd /
rm -rf "$work"
