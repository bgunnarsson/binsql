#!/usr/bin/env bash
set -euo pipefail

# Config
PKG_NAME="binsql"
ARCH="amd64"
MAINTAINER_NAME="Brynjólfur Gunnarsson"
MAINTAINER_EMAIL="you@example.com"   # change this

ROOT_DIR="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT_DIR"

if ! command -v dpkg-deb >/dev/null; then
  echo "dpkg-deb is required" >&2
  exit 1
fi

if ! command -v dpkg-scanpackages >/dev/null; then
  echo "dpkg-scanpackages (dpkg-dev) is required" >&2
  exit 1
fi

if ! command -v apt-ftparchive >/dev/null; then
  echo "apt-ftparchive is required (usually in apt-utils)" >&2
  exit 1
fi

VERSION="${1:-$(git describe --tags --abbrev=0 | sed 's/^v//')}"
echo "Version: $VERSION"

# 1) Build linux binary
BIN_OUT="dist/${PKG_NAME}-linux-${ARCH}"
mkdir -p dist
echo "Building Go binary..."
GOOS=linux GOARCH="$ARCH" go build -o "$BIN_OUT" ./cmd/binsql

# 2) Build .deb
PKGROOT="dist/apt/${PKG_NAME}_${VERSION}_${ARCH}"
DEB_OUT="dist/apt/${PKG_NAME}_${VERSION}_${ARCH}.deb"

echo "Preparing deb root: $PKGROOT"
rm -rf "$PKGROOT"
mkdir -p \
  "$PKGROOT/DEBIAN" \
  "$PKGROOT/usr/bin" \
  "$PKGROOT/usr/share/doc/$PKG_NAME"

cat > "$PKGROOT/DEBIAN/control" <<EOF
Package: $PKG_NAME
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCH
Maintainer: $MAINTAINER_NAME <$MAINTAINER_EMAIL>
Description: TUI SQL client for SQLite, Postgres, MSSQL and MySQL.
EOF

install -m755 "$BIN_OUT" "$PKGROOT/usr/bin/$PKG_NAME"

# optional: README / LICENSE if present
[ -f README.md ] && install -m644 README.md "$PKGROOT/usr/share/doc/$PKG_NAME/README.md"
[ -f LICENSE ] && install -m644 LICENSE "$PKGROOT/usr/share/doc/$PKG_NAME/LICENSE"

echo "Building deb: $DEB_OUT"
mkdir -p "$(dirname "$DEB_OUT")"
dpkg-deb --build "$PKGROOT" "$DEB_OUT"

# 3) Minimal apt repo
REPO_ROOT="dist/apt/repo"
mkdir -p "$REPO_ROOT"
cp "$DEB_OUT" "$REPO_ROOT/"

pushd "$REPO_ROOT" >/dev/null
echo "Generating Packages / Release..."
dpkg-scanpackages . /dev/null > Packages
gzip -f -k Packages
apt-ftparchive release . > Release
popd >/dev/null

cat <<EOF

Done.

Deb file:
  $DEB_OUT

Local apt repo root:
  $REPO_ROOT

Example source entry (for users):

  deb [trusted=yes] https://YOUR_DOMAIN/PATH_TO/repo ./

EOF

