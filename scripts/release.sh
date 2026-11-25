#!/usr/bin/env bash
set -euo pipefail

SKIP_VERIFY=0
for arg in "$@"; do
  case "$arg" in
    --skip-verify) SKIP_VERIFY=1 ;;
    *)
      echo "Unknown option: $arg" >&2
      echo "Usage: $0 [--skip-verify]" >&2
      exit 1
      ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

VERSION="$(node -p "require('./package.json').version")"
if [[ -z "${VERSION}" ]]; then
  echo "Could not determine version from package.json" >&2
  exit 1
fi

if [[ "${SKIP_VERIFY}" -eq 0 ]]; then
  echo "==> Local verification (pnpm verify)"
  pnpm verify
else
  echo "==> Skipping pnpm verify (--skip-verify)"
fi

echo "==> Build staff-sync-server release binary (pnpm build:server)"
pnpm build:server

echo "==> Build Ferrule desktop installers (pnpm tauri build)"
pnpm tauri build

OUT_DIR="${REPO_ROOT}/release-artifacts/v${VERSION}"
rm -rf "${OUT_DIR}"
mkdir -p "${OUT_DIR}"

UNAME_S="$(uname -s | tr '[:upper:]' '[:lower:]')"
case "${UNAME_S}" in
  darwin*) OS_NAME="macos" ;;
  linux*)  OS_NAME="linux" ;;
  *)       OS_NAME="${UNAME_S}" ;;
esac

UNAME_M="$(uname -m)"
case "${UNAME_M}" in
  x86_64|amd64) ARCH_NAME="x64" ;;
  aarch64|arm64) ARCH_NAME="arm64" ;;
  *) ARCH_NAME="${UNAME_M}" ;;
esac

BUNDLE_ROOT="${REPO_ROOT}/target/release/bundle"
shopt -s nullglob
for file in \
  "${BUNDLE_ROOT}"/dmg/*.dmg \
  "${BUNDLE_ROOT}"/appimage/*.AppImage \
  "${BUNDLE_ROOT}"/deb/*.deb \
  "${BUNDLE_ROOT}"/rpm/*.rpm
do
  cp -f "${file}" "${OUT_DIR}/$(basename "${file}")"
done
shopt -u nullglob

SERVER_BIN="${REPO_ROOT}/target/release/staff-sync-server"
if [[ -f "${SERVER_BIN}" ]]; then
  cp -f "${SERVER_BIN}" "${OUT_DIR}/staff-sync-server-v${VERSION}-${OS_NAME}-${ARCH_NAME}"
else
  echo "Expected staff-sync-server binary not found at ${SERVER_BIN}" >&2
  exit 1
fi

(
  cd "${OUT_DIR}"
  FILES=(*)
  if [[ "${#FILES[@]}" -eq 0 ]]; then
    echo "No release artifacts were found to stage." >&2
    exit 1
  fi

  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${FILES[@]}" | sort -k2 > SHA256SUMS.txt
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "${FILES[@]}" | sort -k2 > SHA256SUMS.txt
  else
    echo "Neither sha256sum nor shasum found in PATH" >&2
    exit 1
  fi
)

echo ""
echo "==> Staged Ferrule v${VERSION} release artifacts in: ${OUT_DIR}"
sed 's/^/    /' "${OUT_DIR}/SHA256SUMS.txt"
