#!/usr/bin/env bash
# 构建 .deb：需要先 cargo build --release
# 用法: ./packaging/deb/build-deb.sh <version>
set -euo pipefail

VERSION="${1:?用法: build-deb.sh <version>}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PKG="yd-translate"
ARCH="amd64"
OUT="$ROOT/dist"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

for bin in yd yd-gui; do
  [[ -x "$ROOT/target/release/$bin" ]] || {
    echo "缺少 target/release/$bin，请先 cargo build --release" >&2
    exit 1
  }
done

install -Dm755 "$ROOT/target/release/yd" "$STAGE/usr/bin/yd"
install -Dm755 "$ROOT/target/release/yd-gui" "$STAGE/usr/bin/yd-gui"
install -Dm644 "$ROOT/packaging/common/yd-gui.desktop" "$STAGE/usr/share/applications/yd-gui.desktop"
install -Dm644 "$ROOT/packaging/common/yd-gui.png" "$STAGE/usr/share/icons/hicolor/512x512/apps/yd-gui.png"
install -Dm644 "$ROOT/LICENSE" "$STAGE/usr/share/doc/$PKG/copyright"
install -Dm644 "$ROOT/README.md" "$STAGE/usr/share/doc/$PKG/README.md"

mkdir -p "$STAGE/DEBIAN"
INSTALLED_SIZE="$(du -sk "$STAGE/usr" | cut -f1)"
cat > "$STAGE/DEBIAN/control" <<CONTROL
Package: $PKG
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCH
Installed-Size: $INSTALLED_SIZE
Maintainer: totrytakeoff <totrytakeoff@users.noreply.github.com>
Depends: libc6, libgtk-3-0, libwebkit2gtk-4.1-0
Homepage: https://github.com/totrytakeoff/yd-translate
Description: Youdao translate CLI and floating window
 有道翻译命令行（yd）与 Tauri 浮动窗（yd-gui），支持词典释义、
 整句翻译与图片 OCR。
CONTROL

mkdir -p "$OUT"
dpkg-deb --build --root-owner-group "$STAGE" "$OUT/${PKG}_${VERSION}_${ARCH}.deb"
echo "built: $OUT/${PKG}_${VERSION}_${ARCH}.deb"
