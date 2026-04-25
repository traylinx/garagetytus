#!/usr/bin/env bash
# garagetytus web bootstrap — Linux + macOS.
#
# Downloads the appropriate `garagetytus` release binary from
# github.com/traylinx/garagetytus/releases/latest and places it on
# PATH. After this script finishes, the user runs:
#
#     garagetytus install   # cross-platform Garage acquisition
#     garagetytus start
#     garagetytus bootstrap
#
# This bootstrapper itself does NOT touch Garage — `garagetytus
# install` does that. Idempotent. Re-runnable.

set -euo pipefail

REPO="traylinx/garagetytus"
INSTALL_DIR_DEFAULT="${HOME}/.local/bin"
INSTALL_DIR="${GARAGETYTUS_INSTALL_DIR:-${INSTALL_DIR_DEFAULT}}"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}" in
    Darwin)
        case "${ARCH}" in
            arm64) TARGET="aarch64-apple-darwin" ;;
            x86_64) TARGET="x86_64-apple-darwin" ;;
            *) echo "garagetytus: unsupported macOS arch: ${ARCH}" >&2; exit 1 ;;
        esac
        ;;
    Linux)
        case "${ARCH}" in
            x86_64) TARGET="x86_64-unknown-linux-musl" ;;
            aarch64|arm64) TARGET="aarch64-unknown-linux-musl" ;;
            *) echo "garagetytus: unsupported Linux arch: ${ARCH}" >&2; exit 1 ;;
        esac
        ;;
    *)
        echo "garagetytus: unsupported OS: ${OS}" >&2
        echo "  v0.1 supports macOS + Linux. Windows targets v0.2." >&2
        exit 1
        ;;
esac

echo "garagetytus bootstrap: target=${TARGET}, install_dir=${INSTALL_DIR}"

mkdir -p "${INSTALL_DIR}"

# Latest release URL pattern follows cargo-dist convention. The
# release artifact name + tarball layout will be confirmed by the
# Phase B release wiring.
URL="https://github.com/${REPO}/releases/latest/download/garagetytus-${TARGET}.tar.gz"
TMPDIR_BOOT="$(mktemp -d -t garagetytus-bootstrap.XXXXXX)"
trap 'rm -rf "${TMPDIR_BOOT}"' EXIT

echo "garagetytus bootstrap: downloading ${URL}"
if ! curl -fsSL --retry 3 --retry-connrefused -o "${TMPDIR_BOOT}/g.tar.gz" "${URL}"; then
    echo "garagetytus bootstrap: download failed." >&2
    echo "  Check https://github.com/${REPO}/releases for available builds." >&2
    exit 1
fi

tar -xzf "${TMPDIR_BOOT}/g.tar.gz" -C "${TMPDIR_BOOT}"
BIN_PATH="$(find "${TMPDIR_BOOT}" -maxdepth 3 -name 'garagetytus' -type f | head -n1)"
if [[ -z "${BIN_PATH}" ]]; then
    echo "garagetytus bootstrap: extracted archive missing the binary." >&2
    exit 1
fi

install -m 0755 "${BIN_PATH}" "${INSTALL_DIR}/garagetytus"
echo "garagetytus bootstrap: installed ${INSTALL_DIR}/garagetytus"

if ! command -v garagetytus >/dev/null 2>&1; then
    echo "garagetytus bootstrap: ${INSTALL_DIR} is not on PATH yet."
    echo "  Add it with:"
    echo "      export PATH=\"${INSTALL_DIR}:\$PATH\""
    echo "  …then re-open the shell, or run \`${INSTALL_DIR}/garagetytus install\` directly."
    exit 0
fi

echo
echo "garagetytus bootstrap: done. Next steps:"
echo "  garagetytus install"
echo "  garagetytus start"
echo "  garagetytus bootstrap"
