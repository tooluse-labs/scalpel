#!/bin/sh
set -eu

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <Scalpel.app> <output.dmg> <version>" >&2
  exit 2
fi

app_input=$1
dmg_input=$2
version=$3

app_dir=$(cd "$(dirname "$app_input")" && pwd)
app_path="$app_dir/$(basename "$app_input")"
if [ ! -d "$app_path" ]; then
  echo "app bundle was not found: $app_path" >&2
  exit 1
fi

dmg_dir=$(dirname "$dmg_input")
mkdir -p "$dmg_dir"
dmg_dir=$(cd "$dmg_dir" && pwd)
dmg_path="$dmg_dir/$(basename "$dmg_input")"

volume_name="Scalpel ${version}"
window_width=560
window_height=360
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/scalpel-dmg.XXXXXX")
mount_dir="$work_dir/mount"
rw_dmg="$work_dir/scalpel-rw.dmg"
swift_cache="$work_dir/swift-cache"
swift_script="$work_dir/dmg-background.swift"
attached=0
app_size_kb=$(du -sk "$app_path" | awk '{print $1}')
dmg_size_mb=$((app_size_kb / 1024 + 160))
if [ "$dmg_size_mb" -lt 320 ]; then
  dmg_size_mb=320
fi

cleanup() {
  if [ "$attached" -eq 1 ]; then
    hdiutil detach "$mount_dir" -quiet >/dev/null 2>&1 \
      || hdiutil detach "$mount_dir" -force -quiet >/dev/null 2>&1 \
      || true
  fi
  rm -rf "$work_dir"
}
trap cleanup EXIT INT TERM

cat > "$swift_script" <<'SWIFT'
import AppKit

let out = CommandLine.arguments[1]
let width: CGFloat = 560
let height: CGFloat = 360
let image = NSImage(size: NSSize(width: width, height: height))

func color(_ red: CGFloat, _ green: CGFloat, _ blue: CGFloat, _ alpha: CGFloat = 1.0) -> NSColor {
    NSColor(calibratedRed: red, green: green, blue: blue, alpha: alpha)
}

func fillRoundedRect(_ rect: NSRect, radius: CGFloat, color fill: NSColor) {
    fill.setFill()
    NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius).fill()
}

func drawString(_ value: String, x: CGFloat, y: CGFloat, size: CGFloat, weight: NSFont.Weight, color textColor: NSColor) {
    let attributes: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: size, weight: weight),
        .foregroundColor: textColor
    ]
    (value as NSString).draw(at: NSPoint(x: x, y: y), withAttributes: attributes)
}

image.lockFocus()

color(0.965, 0.970, 0.955).setFill()
NSBezierPath(rect: NSRect(x: 0, y: 0, width: width, height: height)).fill()

fillRoundedRect(NSRect(x: 18, y: 18, width: width - 36, height: height - 36), radius: 22, color: color(1.0, 1.0, 1.0, 0.80))
fillRoundedRect(NSRect(x: 104, y: 126, width: 124, height: 124), radius: 24, color: color(0.89, 0.93, 0.96, 0.55))
fillRoundedRect(NSRect(x: 332, y: 126, width: 124, height: 124), radius: 24, color: color(0.89, 0.93, 0.96, 0.55))

drawString("Install Scalpel", x: 198, y: 292, size: 25, weight: .semibold, color: color(0.12, 0.13, 0.14))
drawString("Drag Scalpel to Applications", x: 178, y: 264, size: 14, weight: .regular, color: color(0.35, 0.38, 0.40))

let arrow = NSBezierPath()
arrow.move(to: NSPoint(x: 246, y: 188))
arrow.line(to: NSPoint(x: 314, y: 188))
arrow.lineWidth = 4
arrow.lineCapStyle = .round
color(0.12, 0.46, 0.54).setStroke()
arrow.stroke()

let arrowHead = NSBezierPath()
arrowHead.move(to: NSPoint(x: 314, y: 188))
arrowHead.line(to: NSPoint(x: 296, y: 202))
arrowHead.line(to: NSPoint(x: 296, y: 174))
arrowHead.close()
color(0.12, 0.46, 0.54).setFill()
arrowHead.fill()

image.unlockFocus()

guard let tiff = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiff),
      let png = bitmap.representation(using: .png, properties: [:]) else {
    fatalError("failed to render DMG background")
}

try png.write(to: URL(fileURLWithPath: out))
SWIFT

mkdir -p "$mount_dir" "$swift_cache"
rm -f "$dmg_path"
printf 'creating DMG working image: %sm\n' "$dmg_size_mb"
hdiutil create -quiet -fs HFS+ -volname "$volume_name" -size "${dmg_size_mb}m" "$rw_dmg"
hdiutil attach "$rw_dmg" -quiet -mountpoint "$mount_dir" -noverify
attached=1

ditto "$app_path" "$mount_dir/Scalpel.app"
ln -s /Applications "$mount_dir/Applications"
mkdir -p "$mount_dir/.background"
CLANG_MODULE_CACHE_PATH="$swift_cache" swift "$swift_script" "$mount_dir/.background/background.png"
chflags hidden "$mount_dir/.background" >/dev/null 2>&1 || true

if ! osascript <<APPLESCRIPT
tell application "Finder"
  set volumeFolder to POSIX file "$mount_dir" as alias
  open volumeFolder
  delay 1
  set volumeWindow to container window of volumeFolder
  set current view of volumeWindow to icon view
  set toolbar visible of volumeWindow to false
  set statusbar visible of volumeWindow to false
  set bounds of volumeWindow to {100, 100, 100 + $window_width, 100 + $window_height}
  set viewOptions to icon view options of volumeWindow
  set arrangement of viewOptions to not arranged
  set icon size of viewOptions to 96
  set backgroundFile to POSIX file "$mount_dir/.background/background.png" as alias
  set background picture of viewOptions to backgroundFile
  try
    set position of item "Scalpel.app" of volumeFolder to {165, 176}
  on error
    set position of item "Scalpel" of volumeFolder to {165, 176}
  end try
  set position of item "Applications" of volumeFolder to {395, 176}
  update volumeFolder without registering applications
  delay 1
  close volumeWindow
end tell
APPLESCRIPT
then
  echo "warning: Finder DMG layout customization failed; continuing with default DMG layout" >&2
fi

sync
sleep 1

for _attempt in 1 2 3 4 5; do
  if hdiutil detach "$mount_dir" -quiet >/dev/null 2>&1; then
    attached=0
    break
  fi
  sleep 1
done

if [ "$attached" -eq 1 ]; then
  hdiutil detach "$mount_dir" -force -quiet
  attached=0
fi

hdiutil convert "$rw_dmg" -quiet -format UDZO -imagekey zlib-level=9 -o "$dmg_path"
printf 'packaged DMG: %s\n' "$dmg_path"
