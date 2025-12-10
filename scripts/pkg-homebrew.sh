#!/usr/bin/env bash
set -euo pipefail

# Config
OWNER="bgunnarsson"
REPO="binsql"
FORMULA_FILE="binsql.rb"   # change if your formula lives elsewhere

if [ ! -f "$FORMULA_FILE" ]; then
  echo "Formula file not found: $FORMULA_FILE" >&2
  exit 1
fi

if [ $# -lt 1 ]; then
  echo "Usage: $0 <version-without-v>  (e.g. $0 1.0.0)" >&2
  exit 1
fi

VERSION="$1"
TAG="v${VERSION}"
TARBALL_URL="https://github.com/${OWNER}/${REPO}/archive/refs/tags/${TAG}.tar.gz"

echo "Updating Homebrew formula to ${TAG}"
echo "Tarball: ${TARBALL_URL}"

# Fetch tarball and compute sha256
TMP_TGZ="$(mktemp)"
echo "Downloading tarball..."
curl -L -sSf "$TARBALL_URL" -o "$TMP_TGZ"

SHA256="$(shasum -a 256 "$TMP_TGZ" | awk '{print $1}')"
rm -f "$TMP_TGZ"

echo "sha256: ${SHA256}"

# Update url and sha256 lines in the formula
# expects lines like:
#   url "https://github.com/bgunnarsson/binsql/archive/refs/tags/v0.1.0.tar.gz"
#   sha256 "...."
perl -pi -e 's|^  url ".*"|  url "'"${TARBALL_URL}"'"|' "$FORMULA_FILE"
perl -pi -e 's|^  sha256 ".*"|  sha256 "'"${SHA256}"'"|' "$FORMULA_FILE"

echo "Updated ${FORMULA_FILE}:"
grep -E 'url "|sha256 "' "$FORMULA_FILE"

# Git commit + push
git status --short

read -r -p "Commit and push these changes? [y/N] " ans
if [[ "$ans" =~ ^[Yy]$ ]]; then
  git add "$FORMULA_FILE"
  git commit -m "binsql ${VERSION}"
  git push
  echo "Pushed updated formula."
else
  echo "Aborted before commit."
fi

cat <<EOF

Next steps (on your dev machine):

  brew uninstall binsql        # if installed from this tap
  brew untap ${OWNER}/${REPO} || true
  brew tap ${OWNER}/${REPO}
  brew install ${OWNER}/${REPO}/binsql

EOF

