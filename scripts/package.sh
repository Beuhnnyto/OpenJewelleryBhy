#!/usr/bin/env bash
# Builds release packages into dist/:
#   OpenJewelleryBhy-<v>-windows-x64-setup.exe     Windows installer
#   OpenJewelleryBhy-<v>-windows-x64-portable.zip  Windows, no install
#   OpenJewelleryBhy-<v>-linux-x64.tar.gz          Linux (install.sh inside)
# Windows binaries are cross-compiled with cargo-zigbuild (needs `zig`).
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
WIN=x86_64-pc-windows-gnu
DIST=dist
STAGE=target/package
rm -rf "$STAGE" && mkdir -p "$STAGE/win" "$STAGE/linux" "$DIST"

echo "==> Linux release build"
cargo build --release -p ojb-app -p ojb-cli

echo "==> Windows release build ($WIN)"
rustup target add "$WIN" >/dev/null
cargo zigbuild --release --target "$WIN" -p ojb-app -p ojb-cli

echo "==> Icons"
./target/release/ojb icon --ico "$STAGE/win/ojb.ico" --png "$STAGE/linux/openjewellerybhy.png"

echo "==> Windows payload"
cp "target/$WIN/release/openjewellerybhy.exe" "target/$WIN/release/ojb.exe" README.md LICENSE "$STAGE/win/"
cp -r docs "$STAGE/win/docs"
PAYLOAD="$PWD/$STAGE/payload.zip"
python3 - "$STAGE/win" "$PAYLOAD" <<'PY'
import os, sys, zipfile
src, out = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for root, _, files in os.walk(src):
        for f in sorted(files):
            p = os.path.join(root, f)
            z.write(p, os.path.relpath(p, src))
PY
cp "$PAYLOAD" "$DIST/OpenJewelleryBhy-$VERSION-windows-x64-portable.zip"

echo "==> Windows installer"
OJB_PAYLOAD_ZIP="$PAYLOAD" cargo zigbuild --release --target "$WIN" -p ojb-setup
cp "target/$WIN/release/OpenJewelleryBhy-setup.exe" "$DIST/OpenJewelleryBhy-$VERSION-windows-x64-setup.exe"

echo "==> Linux archive"
L="$STAGE/linux"
cp target/release/openjewellerybhy target/release/ojb README.md LICENSE packaging/linux/install.sh packaging/linux/uninstall.sh "$L/"
chmod +x "$L/install.sh" "$L/uninstall.sh"
tar -C "$STAGE" -czf "$DIST/OpenJewelleryBhy-$VERSION-linux-x64.tar.gz" --transform "s|^linux|OpenJewelleryBhy-$VERSION|" linux

(cd "$DIST" && sha256sum OpenJewelleryBhy-$VERSION-* > SHA256SUMS-$VERSION.txt)
echo "==> Done"
ls -lh "$DIST"
