#!/bin/sh
# Installs OpenJewelleryBhy for the current user (no root needed).
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
PREFIX="${PREFIX:-$HOME/.local}"
DEST="$PREFIX/opt/OpenJewelleryBhy"
mkdir -p "$DEST" "$PREFIX/bin" "$PREFIX/share/applications" "$PREFIX/share/icons/hicolor/256x256/apps" "$PREFIX/share/mime/packages"
cp "$HERE/openjewellerybhy" "$HERE/ojb" "$DEST/"
cp "$HERE/README.md" "$HERE/LICENSE" "$HERE/uninstall.sh" "$DEST/" 2>/dev/null || true
chmod +x "$DEST/openjewellerybhy" "$DEST/ojb" "$DEST/uninstall.sh"
ln -sf "$DEST/openjewellerybhy" "$PREFIX/bin/openjewellerybhy"
ln -sf "$DEST/ojb" "$PREFIX/bin/ojb"
cp "$HERE/openjewellerybhy.png" "$PREFIX/share/icons/hicolor/256x256/apps/openjewellerybhy.png"
cat > "$PREFIX/share/applications/openjewellerybhy.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=OpenJewelleryBhy
Comment=Parametric jewellery CAD
Exec=$DEST/openjewellerybhy %f
Icon=openjewellerybhy
Terminal=false
Categories=Graphics;3DGraphics;Engineering;
MimeType=application/x-openjewellerybhy;
StartupWMClass=openjewellerybhy
DESKTOP
cat > "$PREFIX/share/mime/packages/openjewellerybhy.xml" <<MIME
<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="application/x-openjewellerybhy">
    <comment>OpenJewelleryBhy design</comment>
    <glob pattern="*.ojb"/>
    <icon name="openjewellerybhy"/>
  </mime-type>
</mime-info>
MIME
command -v update-mime-database >/dev/null && update-mime-database "$PREFIX/share/mime" >/dev/null 2>&1 || true
command -v update-desktop-database >/dev/null && update-desktop-database "$PREFIX/share/applications" >/dev/null 2>&1 || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$PREFIX/share/icons/hicolor" >/dev/null 2>&1 || true
echo "OpenJewelleryBhy installed in $DEST"
echo "Launch it from your applications menu or with: openjewellerybhy"
case ":$PATH:" in *":$PREFIX/bin:"*) ;; *) echo "Note: add $PREFIX/bin to your PATH to use the 'ojb' command line.";; esac
