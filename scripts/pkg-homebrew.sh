#!/usr/bin/env bash
set -euo pipefail

# --- Config -------------------------------------------------------------

OWNER="bgunnarsson"
REPO="binsql"

# Resolve paths relative to this script
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
SRC_REPO_DIR="${SCRIPT_DIR}/.."             # binsql repo
TAP_DIR="${SCRIPT_DIR}/../../homebrew-binsql"

# --- Args ---------------------------------------------------------------

if [ $# -lt 1 ]; then
  echo "Usage: $0 <version-without-v>  (e.g. $0 2.0.0)" >&2
  exit 1
fi

VERSION="$1"
TAG="v${VERSION}"
TARBALL_URL="https://github.com/${OWNER}/${REPO}/archive/refs/tags/${TAG}.tar.gz"

# --- Sanity checks ------------------------------------------------------

if [ ! -d "$SRC_REPO_DIR" ]; then
  echo "Source repo dir not found: $SRC_REPO_DIR" >&2
  exit 1
fi

if [ ! -d "$TAP_DIR" ]; then
  echo "Homebrew tap repo not found: $TAP_DIR" >&2
  exit 1
fi

# Try common formula locations
CANDIDATES=(
  "${TAP_DIR}/binsql.rb"
  "${TAP_DIR}/Formula/binsql.rb"
  "${TAP_DIR}/HomebrewFormula/binsql.rb"
)

FORMULA_FILE=""
for f in "${CANDIDATES[@]}"; do
  if [ -f "$f" ]; then
    FORMULA_FILE="$f"
    break
  fi
done

if [ -z "$FORMULA_FILE" ]; then
  echo "Formula file not found in any of:" >&2
  printf '  %s\n' "${CANDIDATES[@]}" >&2
  exit 1
fi

echo "Releasing binsql ${VERSION}"
echo "  src repo:     ${SRC_REPO_DIR}"
echo "  tap repo:     ${TAP_DIR}"
echo "  formula file: ${FORMULA_FILE}"
echo "  tag:          ${TAG}"
echo "  tarball:      ${TARBALL_URL}"
echo

# --- Step 1: ensure git tag exists and is pushed ------------------------

cd "$SRC_REPO_DIR"

# require clean working tree to avoid tagging dirty state
if ! git diff-index --quiet HEAD --; then
  echo "Working tree in ${SRC_REPO_DIR} is not clean. Commit or stash first." >&2
  exit 1
fi

# fetch tags so we see remote state
git fetch --tags >/dev/null 2>&1 || true

if git rev-parse "${TAG}" >/dev/null 2>&1; then
  echo "Tag ${TAG} already exists locally."
else
  echo "Creating tag ${TAG}..."
  git tag -a "${TAG}" -m "binsql ${VERSION}"
fi

echo "Pushing tag ${TAG} to origin..."
git push origin "${TAG}"

echo
echo "Tag pushed. GitHub tarball should be available at:"
echo "  ${TARBALL_URL}"
echo

# --- Step 2: compute tarball sha256 -------------------------------------

TMP_TGZ="$(mktemp)"
echo "Downloading tarball..."
curl -L -sSf "$TARBALL_URL" -o "$TMP_TGZ"

SHA256="$(shasum -a 256 "$TMP_TGZ" | awk '{print $1}')"
rm -f "$TMP_TGZ"

echo "sha256: ${SHA256}"
echo

# --- Step 3: update Homebrew formula ------------------------------------

perl -pi -e 's|^  url ".*"|  url "'"${TARBALL_URL}"'"|' "$FORMULA_FILE"
perl -pi -e 's|^  sha256 ".*"|  sha256 "'"${SHA256}"'"|' "$FORMULA_FILE"

echo "Updated ${FORMULA_FILE}:"
grep -E 'url "|sha256 "' "$FORMULA_FILE" || true
echo

# --- Step 4: commit + push tap repo -------------------------------------

cd "$TAP_DIR"
echo "Git status in tap repo (${TAP_DIR}):"
git status --short
echo

read -r -p "Commit and push these Homebrew changes? [y/N] " ans
if [[ "$ans" =~ ^[Yy]$ ]]; then
  git add "$FORMULA_FILE"
  git commit -m "binsql ${VERSION}"
  git push
  echo "Pushed updated formula."
else
  echo "Aborted before commit."
fi

cat <<EOF

Next steps:

  brew uninstall binsql        # if installed from this tap
  brew untap ${OWNER}/${REPO} || true
  brew tap ${OWNER}/${REPO}
  brew install ${OWNER}/${REPO}/binsql

EOF
