#!/bin/sh
# Removes OpenJewelleryBhy installed by install.sh (designs are kept).
PREFIX="${PREFIX:-$HOME/.local}"
rm -f "$PREFIX/bin/openjewellerybhy" "$PREFIX/bin/ojb" \
      "$PREFIX/share/applications/openjewellerybhy.desktop" \
      "$PREFIX/share/icons/hicolor/256x256/apps/openjewellerybhy.png" \
      "$PREFIX/share/mime/packages/openjewellerybhy.xml"
rm -rf "$PREFIX/opt/OpenJewelleryBhy"
command -v update-mime-database >/dev/null && update-mime-database "$PREFIX/share/mime" >/dev/null 2>&1 || true
command -v update-desktop-database >/dev/null && update-desktop-database "$PREFIX/share/applications" >/dev/null 2>&1 || true
echo "OpenJewelleryBhy removed."
