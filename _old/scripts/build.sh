#!/usr/bin/env bash
set -euo pipefail

APP="binsql"
PKG="./cmd/binsql"   # change if your main is somewhere else
OUT_DIR="./dist"

# Stamped into the binary and reported by `binsql version`. Defaults to the
# current git tag/commit, override with VERSION=x.y.z ./scripts/build.sh
VERSION="${VERSION:-$(git describe --tags --always --dirty 2>/dev/null || echo dev)}"

mkdir -p "$OUT_DIR"

# disable cgo to make cross-compiles trivial
export CGO_ENABLED=0

build() {
  local goos="$1"
  local goarch="$2"
  local ext=""
  local bin="${APP}-${goos}-${goarch}"

  if [ "$goos" = "windows" ]; then
    ext=".exe"
  fi

  echo "==> $goos/$goarch"
  GOOS="$goos" GOARCH="$goarch" \
    go build \
      -trimpath \
      -ldflags="-s -w -X main.version=${VERSION}" \
      -o "${OUT_DIR}/${bin}${ext}" \
      "$PKG"
}

echo "building ${APP} ${VERSION}"

# Apple Silicon
build darwin arm64

# Apple Intel
build darwin amd64

# Linux (64-bit)
build linux amd64
build linux arm64

# Windows (64-bit)
build windows amd64

