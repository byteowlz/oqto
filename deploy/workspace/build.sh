#!/usr/bin/env bash
# Build the oqto-workspace OCI image with release provenance labels.
#
# Labels stamped:
#   org.opencontainers.image.version = OQTO_VERSION  (Cargo release semver)
#   org.opencontainers.image.revision = OQTO_REVISION (release commit SHA)
#   org.opencontainers.image.source   = repo URL
#   io.oqto.role    = "workspace"
#   io.oqto.version = OQTO_VERSION  (backend compatibility key)
#
# A local `:dev` build (no --release) leaves these empty, which the backend
# treats as an unattested image accepted only in dev placements.
#
# Usage:
#   deploy/workspace/build.sh                     # localhost/oqto-workspace:dev
#   deploy/workspace/build.sh --release           # ghcr.io/byteowlz/oqto-workspace:<ver>
#   deploy/workspace/build.sh --tag vX.Y.Z --revision <sha>
#   deploy/workspace/build.sh --smoke             # run test-image.sh after build
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT_DIR"

REGISTRY="${REGISTRY:-ghcr.io}"
IMAGE_NAME="${IMAGE_NAME:-byteowlz/oqto-workspace}"
SOURCE_URL="${SOURCE_URL:-https://github.com/byteowlz/oqto}"
ARCH="${ARCH:-$(uname -m)}"

OQTO_VERSION=""
OQTO_REVISION=""
RELEASE=false
RUN_SMOKE=false
EXTRA_TAG=""

usage() {
  cat <<EOF
Usage: $0 [--release] [--tag <semver>] [--revision <sha>] [--smoke]
          [--registry <reg>] [--image-name <name>]

  --release       Stamp version+revision from the working tree and tag the
                  release image (defaults: version from backend/Cargo.toml,
                  revision from git rev-parse HEAD).
  --tag <semver>  Explicit version (e.g. 0.5.0). Implies --release.
  --revision <s>  Explicit source revision SHA. Implies --release.
  --smoke         Run deploy/workspace/test-image.sh after building.
  --registry, --image-name, --source-url override the published reference.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release) RELEASE=true; shift ;;
    --tag) OQTO_VERSION="$2"; RELEASE=true; shift 2 ;;
    --revision) OQTO_REVISION="$2"; RELEASE=true; shift 2 ;;
    --smoke) RUN_SMOKE=true; shift ;;
    --registry) REGISTRY="$2"; shift 2 ;;
    --image-name) IMAGE_NAME="$2"; shift 2 ;;
    --source-url) SOURCE_URL="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if $RELEASE; then
  if [[ -z "$OQTO_VERSION" ]]; then
    OQTO_VERSION="$(grep -m1 '^version = ' backend/Cargo.toml | sed 's/version = "\(.*\)".*/\1/')"
  fi
  if [[ -z "$OQTO_REVISION" ]]; then
    OQTO_REVISION="$(git -C "$ROOT_DIR" rev-parse HEAD)"
  fi
  : "${OQTO_VERSION:?could not derive release version}"
  : "${OQTO_REVISION:?could not derive release revision}"
fi

BUILD_ARGS=(
  --build-arg "OQTO_SOURCE=$SOURCE_URL"
)
if $RELEASE; then
  BUILD_ARGS+=(
    --build-arg "OQTO_VERSION=$OQTO_VERSION"
    --build-arg "OQTO_REVISION=$OQTO_REVISION"
  )
fi

map_targetarch() {
  case "$ARCH" in
    x86_64|amd64) echo "amd64" ;;
    aarch64|arm64) echo "arm64" ;;
    *) echo "unknown" ;;
  esac
}
export TARGETARCH="$(map_targetarch)"

if $RELEASE; then
  TAG="${REGISTRY}/${IMAGE_NAME}:${OQTO_VERSION}"
  echo "==> Building release image: $TAG (version=$OQTO_VERSION revision=$OQTO_REVISION)"
else
  TAG="localhost/oqto-workspace:dev"
  echo "==> Building dev image: $TAG (unattested)"
fi

podman build \
  "${BUILD_ARGS[@]}" \
  -f deploy/workspace/Containerfile \
  -t "$TAG" \
  "$ROOT_DIR"

if $RUN_SMOKE; then
  echo "==> Running image smoke contract"
  OQTO_WORKSPACE_IMAGE="$TAG" bash deploy/workspace/test-image.sh
fi

if $RELEASE; then
  echo "==> Image attestation:"
  podman inspect "$TAG" --format 'version={{ index .Config.Labels "io.oqto.version" }} revision={{ index .Config.Labels "org.opencontainers.image.revision" }} digest={{ .Digest }}'
fi

echo "Done: $TAG"
