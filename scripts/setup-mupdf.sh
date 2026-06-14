#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
VERSION_FILE="$ROOT/third_party/mupdf.version"

if [ ! -f "$VERSION_FILE" ]; then
  echo "missing MuPDF version file: $VERSION_FILE" >&2
  exit 1
fi

. "$VERSION_FILE"

: "${MUPDF_VERSION:?missing MUPDF_VERSION in $VERSION_FILE}"
: "${MUPDF_SOURCE_URL:?missing MUPDF_SOURCE_URL in $VERSION_FILE}"
MUPDF_SHA256="${MUPDF_SHA256:-}"

THIRD_PARTY_DIR="${SCALPEL_MUPDF_THIRD_PARTY_DIR:-$ROOT/third_party}"
CACHE_DIR="${SCALPEL_MUPDF_CACHE_DIR:-$THIRD_PARTY_DIR/cache}"
ARCHIVE="$CACHE_DIR/mupdf-$MUPDF_VERSION-source.tar.gz"
ARCHIVE_PART="$ARCHIVE.part"
SOURCE_DIR="${SCALPEL_MUPDF_SOURCE_DIR:-$THIRD_PARTY_DIR/mupdf-$MUPDF_VERSION-source}"
ENV_FILE="${SCALPEL_MUPDF_ENV_FILE:-$THIRD_PARTY_DIR/mupdf.env}"

mkdir -p "$CACHE_DIR"

archive_sha256() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    echo "shasum or sha256sum is required to verify MuPDF archive checksum" >&2
    exit 1
  fi
}

archive_matches_checksum() {
  [ -f "$ARCHIVE" ] || return 1
  [ -n "$MUPDF_SHA256" ] || return 0
  [ "$(archive_sha256 "$ARCHIVE")" = "$MUPDF_SHA256" ]
}

download_archive() {
  echo "downloading MuPDF $MUPDF_VERSION"
  if [ -f "$ARCHIVE" ] && [ ! -f "$ARCHIVE_PART" ]; then
    mv "$ARCHIVE" "$ARCHIVE_PART"
  fi
  if command -v curl >/dev/null 2>&1; then
    if ! curl -fL --retry 5 --retry-delay 2 -C - "$MUPDF_SOURCE_URL" -o "$ARCHIVE_PART"; then
      rm -f "$ARCHIVE_PART"
      curl -fL --retry 5 --retry-delay 2 "$MUPDF_SOURCE_URL" -o "$ARCHIVE_PART"
    fi
  elif command -v wget >/dev/null 2>&1; then
    wget -c -O "$ARCHIVE_PART" "$MUPDF_SOURCE_URL"
  else
    echo "curl or wget is required to download $MUPDF_SOURCE_URL" >&2
    exit 1
  fi
  mv "$ARCHIVE_PART" "$ARCHIVE"
}

if archive_matches_checksum; then
  echo "using cached archive $ARCHIVE"
else
  download_archive
fi

if [ -n "$MUPDF_SHA256" ]; then
  ACTUAL_SHA256=$(archive_sha256 "$ARCHIVE")
  if [ "$ACTUAL_SHA256" != "$MUPDF_SHA256" ]; then
    echo "MuPDF archive checksum mismatch" >&2
    echo "expected: $MUPDF_SHA256" >&2
    echo "actual:   $ACTUAL_SHA256" >&2
    echo "remove $ARCHIVE and rerun this script to force a clean download" >&2
    exit 1
  fi
fi

if [ ! -d "$SOURCE_DIR" ]; then
  TMP_DIR="$THIRD_PARTY_DIR/.mupdf-extract.$$"
  rm -rf "$TMP_DIR"
  mkdir -p "$TMP_DIR"
  mkdir -p "$(dirname -- "$SOURCE_DIR")"
  echo "extracting MuPDF to $SOURCE_DIR"
  tar -xzf "$ARCHIVE" -C "$TMP_DIR"
  EXTRACTED_DIR=$(find "$TMP_DIR" -mindepth 1 -maxdepth 1 -type d -print | sed -n '1p')
  if [ -z "$EXTRACTED_DIR" ]; then
    echo "MuPDF archive did not contain a source directory" >&2
    rm -rf "$TMP_DIR"
    exit 1
  fi
  mv "$EXTRACTED_DIR" "$SOURCE_DIR"
  rmdir "$TMP_DIR"
else
  echo "using existing source tree $SOURCE_DIR"
fi

for REQUIRED_HEADER in \
  "$SOURCE_DIR/include/mupdf/fitz.h" \
  "$SOURCE_DIR/include/mupdf/pdf.h" \
  "$SOURCE_DIR/include/mupdf/pdf/javascript.h"
do
  if [ ! -f "$REQUIRED_HEADER" ]; then
    echo "MuPDF source tree is missing required header: $REQUIRED_HEADER" >&2
    echo "remove $SOURCE_DIR and rerun this script to extract a clean source tree" >&2
    exit 1
  fi
done

if [ "${SCALPEL_MUPDF_SKIP_BUILD:-0}" != "1" ]; then
  if ! command -v make >/dev/null 2>&1; then
    echo "make is required to build MuPDF; install make or set SCALPEL_MUPDF_SKIP_BUILD=1" >&2
    exit 1
  fi
  echo "building MuPDF release libraries and mutool"
  make -C "$SOURCE_DIR" build=release build/release/mutool
fi

LIB_DIR="$SOURCE_DIR/build/release"
MUTOOL="$LIB_DIR/mutool"
if [ ! -f "$LIB_DIR/libmupdf.a" ] || [ ! -f "$LIB_DIR/libmupdf-third.a" ]; then
  echo "MuPDF static libraries were not found in $LIB_DIR" >&2
  echo "expected libmupdf.a and libmupdf-third.a" >&2
  exit 1
fi
if [ ! -x "$MUTOOL" ]; then
  echo "mutool was not found at $MUTOOL" >&2
  exit 1
fi

cat > "$ENV_FILE" <<EOF
export SCALPEL_MUPDF_SOURCE_DIR="$SOURCE_DIR"
export SCALPEL_MUPDF_INCLUDE_DIR="$SOURCE_DIR/include"
export SCALPEL_MUPDF_LIB_DIR="$LIB_DIR"
export SCALPEL_MUTOOL_PATH="$MUTOOL"
EOF

echo "MuPDF is ready."
echo "Run: . $ENV_FILE"
