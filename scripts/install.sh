#!/usr/bin/env bash
set -euo pipefail

# Bootstrap installer for oqto-setup.
# Downloads oqto-setup binary + sha256 from GitHub Releases, verifies checksum,
# installs oqto-setup, then performs release install from artifact.

REPO="${OQTO_RELEASE_REPO:-byteowlz/oqto}"
BIN_DIR="${BIN_DIR:-/usr/local/bin}"
VERSION="latest"
DRY_RUN="false"
RUN_SETUP_SH="false"

usage() {
  cat <<EOF
Usage: $0 [options] [-- setup-sh-args...]

Options:
  --version <vX.Y.Z|latest>  Release version to install (default: latest)
  --repo <owner/repo>        GitHub repo for releases (default: byteowlz/oqto)
  --bin-dir <dir>            Install directory for oqto-setup (default: /usr/local/bin)
  --run-setup-sh             If ./setup.sh exists, run it after bootstrap+install
  --dry-run                  Print actions without executing
  -h, --help                 Show this help

Examples:
  $0
  $0 --version v0.4.0
  $0 --run-setup-sh -- --personal

Environment:
  OQTO_RELEASE_REPO          Override release repo (owner/repo)
EOF
}

EXTRA_ARGS=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      VERSION="$2"; shift 2 ;;
    --repo)
      REPO="$2"; shift 2 ;;
    --bin-dir)
      BIN_DIR="$2"; shift 2 ;;
    --run-setup-sh)
      RUN_SETUP_SH="true"; shift ;;
    --dry-run)
      DRY_RUN="true"; shift ;;
    -h|--help)
      usage; exit 0 ;;
    --)
      shift
      EXTRA_ARGS=("$@")
      break ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage
      exit 1 ;;
  esac
done

os="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch="$(uname -m)"
case "$arch" in
  x86_64|amd64) arch="x86_64" ;;
  aarch64|arm64) arch="aarch64" ;;
  *)
    echo "error: unsupported architecture: $arch" >&2
    exit 1 ;;
esac

target="${os}-${arch}"

if [[ "$VERSION" == "latest" ]]; then
  VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | python3 -c 'import json,sys; print(json.load(sys.stdin)["tag_name"])')"
fi

setup_asset="oqto-setup-${VERSION}-${target}"
release_asset="oqto-${VERSION}-${target}.tar.gz"
url_base="https://github.com/${REPO}/releases/download/${VERSION}"
url_setup="${url_base}/${setup_asset}"
url_setup_sha="${url_setup}.sha256"
url_release="${url_base}/${release_asset}"
url_release_sha="${url_release}.sha256"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

echo "Installing oqto ${VERSION} (${target}) from ${REPO}"

if [[ "$DRY_RUN" == "true" ]]; then
  echo "[dry-run] curl -fL '$url_setup' -o '$tmpdir/oqto-setup'"
  echo "[dry-run] curl -fL '$url_setup_sha' -o '$tmpdir/oqto-setup.sha256'"
  echo "[dry-run] curl -fL '$url_release' -o '$tmpdir/$release_asset'"
  echo "[dry-run] curl -fL '$url_release_sha' -o '$tmpdir/$release_asset.sha256'"
  echo "[dry-run] install -m 0755 oqto-setup '$BIN_DIR/oqto-setup'"
  echo "[dry-run] sudo '$BIN_DIR/oqto-setup' install --artifact '$tmpdir/$release_asset' --checksum '$tmpdir/$release_asset.sha256'"
  if [[ "$RUN_SETUP_SH" == "true" ]]; then
    echo "[dry-run] ./setup.sh ${EXTRA_ARGS[*]:-}"
  fi
  exit 0
fi

curl -fL "$url_setup" -o "$tmpdir/oqto-setup"
curl -fL "$url_setup_sha" -o "$tmpdir/oqto-setup.sha256"
curl -fL "$url_release" -o "$tmpdir/$release_asset"
curl -fL "$url_release_sha" -o "$tmpdir/$release_asset.sha256"

(
  cd "$tmpdir"
  sha256sum -c oqto-setup.sha256
)

chmod +x "$tmpdir/oqto-setup"

if [[ "$(id -u)" -eq 0 ]]; then
  install -d -m 0755 "$BIN_DIR"
  install -m 0755 "$tmpdir/oqto-setup" "$BIN_DIR/oqto-setup"
  "$BIN_DIR/oqto-setup" install --artifact "$tmpdir/$release_asset" --checksum "$tmpdir/$release_asset.sha256"
else
  sudo install -d -m 0755 "$BIN_DIR"
  sudo install -m 0755 "$tmpdir/oqto-setup" "$BIN_DIR/oqto-setup"
  sudo "$BIN_DIR/oqto-setup" install --artifact "$tmpdir/$release_asset" --checksum "$tmpdir/$release_asset.sha256"
fi

if [[ "$RUN_SETUP_SH" == "true" ]]; then
  if [[ ! -x ./setup.sh ]]; then
    echo "error: --run-setup-sh requested but ./setup.sh not found/executable" >&2
    exit 1
  fi
  ./setup.sh "${EXTRA_ARGS[@]}"
fi

echo "oqto bootstrap install complete"
