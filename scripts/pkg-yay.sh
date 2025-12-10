#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT_DIR"

if ! command -v makepkg >/dev/null; then
  echo "makepkg is required (Arch-based system)" >&2
  exit 1
fi

PKG_NAME="binsql"
VERSION="${1:-$(git describe --tags --abbrev=0 | sed 's/^v//')}"
SRC_URL="https://github.com/bgunnarsson/binsql/archive/refs/tags/v${VERSION}.tar.gz"

# If PKGBUILD doesn’t exist, generate it (same as in pkg-pacman.sh)
if [ ! -f PKGBUILD ]; then
  echo "PKGBUILD not found – generating..."
  cat > PKGBUILD <<EOF
pkgname=${PKG_NAME}
pkgver=${VERSION}
pkgrel=1
pkgdesc="TUI SQL client for SQLite, Postgres, MSSQL and MySQL"
arch=('x86_64' 'aarch64')
url="https://github.com/bgunnarsson/binsql"
license=('MIT')
depends=()
makedepends=('go')
source=("\${pkgname}-\${pkgver}.tar.gz::${SRC_URL}")
sha256sums=('SKIP')

build() {
  cd "\${srcdir}/${PKG_NAME}-\${pkgver}"
  go build -o binsql ./cmd/binsql
}

package() {
  cd "\${srcdir}/${PKG_NAME}-\${pkgver}"
  install -Dm755 binsql "\${pkgdir}/usr/bin/binsql"
  [ -f LICENSE ] && install -Dm644 LICENSE "\${pkgdir}/usr/share/licenses/\${pkgname}/LICENSE"
}
EOF

  echo "Updating sha256sums..."
  SHA_LINE="$(makepkg -g 2>/dev/null | sed -n 's/^sha256sums=//p')"
  if [ -z "$SHA_LINE" ]; then
    echo "Failed to compute sha256sums via makepkg -g" >&2
    exit 1
  fi
  perl -pi -e 's/^sha256sums=.*$/sha256sums='"$SHA_LINE"'/' PKGBUILD
fi

echo "Generating .SRCINFO..."
makepkg --printsrcinfo > .SRCINFO

cat <<EOF

PKGBUILD and .SRCINFO ready for AUR.

Typical next steps (from this directory):

  git init
  git remote add origin ssh://aur@aur.archlinux.org/${PKG_NAME}.git
  git add PKGBUILD .SRCINFO
  git commit -m "binsql ${VERSION}"
  git push -u origin master

Then users can install with:

  yay -S ${PKG_NAME}

EOF

