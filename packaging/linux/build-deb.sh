#!/usr/bin/env bash
# FamilyTree - Debian/Ubuntu .deb paketi (C Launcher uyumlu: paket adı "familytree" sabittir)
# Çıktı: dist/familytree_<sürüm>_amd64.deb
#
# Uygulama tek ikilidir (Rust/Tauri, arayüz içine gömülü): /usr/bin/familytree.
# WebKitGTK ve GTK dağıtımın kendi paketlerinden (Depends) gelir.
#
# Derleme:  bash packaging/linux/build-deb.sh   (Rust, Node.js, WebKitGTK -dev paketleri)
# Kurulum:  sudo apt install ./dist/familytree_<sürüm>_amd64.deb
set -euo pipefail
umask 022

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
# Tek sürüm kaynağı: src-tauri/tauri.conf.json
VERSION="$(node -p 'require("./src-tauri/tauri.conf.json").version')"
[ -n "$VERSION" ] || { echo "Sürüm okunamadı (src-tauri/tauri.conf.json)" >&2; exit 1; }
case "$(uname -m)" in
  x86_64) ;;
  *) echo "yalnız x86_64 paketlenir" >&2; exit 1 ;;
esac

PKG=familytree
OUT="$ROOT/dist/${PKG}_${VERSION}_amd64.deb"

npx tauri build --no-bundle
# cargo'nun hedef klasörü: bir ortam/cargo ayarı onu repo dışına taşıyabilir.
TARGET="$(cargo metadata --format-version 1 --no-deps | node -pe 'JSON.parse(require("fs").readFileSync(0, "utf8")).target_directory')"
BIN="$TARGET/release/familytree"
[ -x "$BIN" ] || { echo "İkili bulunamadı: $BIN" >&2; exit 1; }
"$BIN" --selftest

# Paket ağacı geçici dizinde kurulur: repo izin bitlerini tutmayan bir diskte olabilir;
# dpkg-deb ise DEBIAN/ betiklerinin 0755 olmasını şart koşar.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
STAGE="$TMP/${PKG}_${VERSION}_amd64"
install -d "$STAGE/DEBIAN" "$STAGE/usr/bin" "$STAGE/usr/share/applications" "$STAGE/usr/share/doc/$PKG"

install -m 755 "$BIN" "$STAGE/usr/bin/$PKG"
install -m 644 "$ROOT/packaging/linux/$PKG.desktop" "$STAGE/usr/share/applications/$PKG.desktop"
for pair in "32:32x32.png" "128:128x128.png" "256:128x128@2x.png" "512:icon.png"; do
    size="${pair%%:*}"; file="${pair#*:}"
    d="$STAGE/usr/share/icons/hicolor/${size}x${size}/apps"
    install -d "$d"
    install -m 644 "$ROOT/src-tauri/icons/$file" "$d/$PKG.png"
done
install -d "$STAGE/usr/share/icons/hicolor/scalable/apps"
install -m 644 "$ROOT/src-tauri/icons/icon.svg" "$STAGE/usr/share/icons/hicolor/scalable/apps/$PKG.svg"
install -m 644 "$ROOT/LICENSE" "$STAGE/usr/share/doc/$PKG/copyright"

SIZE_KB="$(du -sk "$STAGE/usr" | cut -f1)"
cat > "$STAGE/DEBIAN/control" <<CTRL
Package: $PKG
Version: $VERSION
Section: utils
Priority: optional
Architecture: amd64
Maintainer: cafercan <cafercan@outlook.com>
Installed-Size: $SIZE_KB
Depends: libwebkit2gtk-4.1-0, libgtk-3-0
Homepage: https://github.com/cafercan/FamilyTree
Description: Family tree and kinship finder from e-Devlet lineage PDFs
 Imports Turkish e-Devlet "Alt-Ust Soy Bilgisi" PDFs into a local database,
 merges relatives across PDFs, finds kinship and cousin marriages and draws
 ancestor/descendant trees. Fully offline: the app has no network access.
CTRL

cat > "$STAGE/DEBIAN/postinst" <<'POST'
#!/bin/sh
set -e
if [ "$1" = "configure" ]; then
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q /usr/share/applications || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
fi
exit 0
POST
cp "$STAGE/DEBIAN/postinst" "$STAGE/DEBIAN/postrm"
sed -i 's/if \[ "\$1" = "configure" \]; then/if [ "$1" = "remove" ] || [ "$1" = "purge" ]; then/' "$STAGE/DEBIAN/postrm"
chmod 755 "$STAGE/DEBIAN/postinst" "$STAGE/DEBIAN/postrm"

mkdir -p "$ROOT/dist"
dpkg-deb --root-owner-group --build "$STAGE" "$OUT"
echo "OK: $OUT"
