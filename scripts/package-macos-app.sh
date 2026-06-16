#!/bin/sh
set -eu

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <scalpel-binary> <output-Scalpel.app> <version>" >&2
  exit 2
fi

binary_input=$1
app_input=$2
version=$3

binary_dir=$(cd "$(dirname "$binary_input")" && pwd)
binary_path="$binary_dir/$(basename "$binary_input")"
if [ ! -f "$binary_path" ]; then
  echo "Scalpel binary was not found: $binary_path" >&2
  exit 1
fi

app_dir=$(dirname "$app_input")
mkdir -p "$app_dir"
app_dir=$(cd "$app_dir" && pwd)
app_path="$app_dir/$(basename "$app_input")"

icon_source="crates/scalpel-app/assets/icons/scalpel-mark.png"
iconset="$app_dir/Scalpel.iconset"
if [ ! -f "$icon_source" ]; then
  echo "icon source was not found: $icon_source" >&2
  exit 1
fi

rm -rf "$app_path" "$iconset"
mkdir -p "$app_path/Contents/MacOS" "$app_path/Contents/Resources" "$iconset"
cp "$binary_path" "$app_path/Contents/MacOS/Scalpel"
chmod 755 "$app_path/Contents/MacOS/Scalpel"
cp README.md LICENSE NOTICES "$app_path/Contents/Resources/"

sips -z 16 16 "$icon_source" --out "$iconset/icon_16x16.png"
sips -z 32 32 "$icon_source" --out "$iconset/icon_16x16@2x.png"
sips -z 32 32 "$icon_source" --out "$iconset/icon_32x32.png"
sips -z 64 64 "$icon_source" --out "$iconset/icon_32x32@2x.png"
sips -z 128 128 "$icon_source" --out "$iconset/icon_128x128.png"
sips -z 256 256 "$icon_source" --out "$iconset/icon_128x128@2x.png"
sips -z 256 256 "$icon_source" --out "$iconset/icon_256x256.png"
sips -z 512 512 "$icon_source" --out "$iconset/icon_256x256@2x.png"
sips -z 512 512 "$icon_source" --out "$iconset/icon_512x512.png"
sips -z 1024 1024 "$icon_source" --out "$iconset/icon_512x512@2x.png"
if ! iconutil -c icns "$iconset" -o "$app_path/Contents/Resources/Scalpel.icns"; then
  echo "iconutil failed; falling back to a single 1024px icns record" >&2
  python3 - "$iconset/icon_512x512@2x.png" "$app_path/Contents/Resources/Scalpel.icns" <<'PY'
import pathlib
import struct
import sys

png = pathlib.Path(sys.argv[1]).read_bytes()
record = b"ic10" + struct.pack(">I", len(png) + 8) + png
pathlib.Path(sys.argv[2]).write_bytes(b"icns" + struct.pack(">I", len(record) + 8) + record)
PY
fi

cat > "$app_path/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDisplayName</key>
  <string>Scalpel</string>
  <key>CFBundleExecutable</key>
  <string>Scalpel</string>
  <key>CFBundleIconFile</key>
  <string>Scalpel</string>
  <key>CFBundleIdentifier</key>
  <string>com.tooluse-labs.scalpel</string>
  <key>CFBundleName</key>
  <string>Scalpel</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${version}</string>
  <key>CFBundleVersion</key>
  <string>${version}</string>
  <key>LSMinimumSystemVersion</key>
  <string>12.0</string>
</dict>
</plist>
EOF

plutil -lint "$app_path/Contents/Info.plist"
printf 'packaged app: %s\n' "$app_path"
