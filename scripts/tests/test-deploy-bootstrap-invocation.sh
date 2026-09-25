#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT
python3 - "$ROOT_DIR/scripts/deploy.sh" "$scratch/deploy-function.sh" <<'PY'
from pathlib import Path
import sys
text = Path(sys.argv[1]).read_text()
name = "deploy_via_oqto_setup_install"
body = text.split(f"\n{name}() {{\n", 1)[1].split("\n}\n", 1)[0]
Path(sys.argv[2]).write_text(f"{name}() {{\n" + body + "\n}\n")
PY
source "$scratch/deploy-function.sh"
err() { printf 'deploy error: %s\n' "$*" >&2; }
# Run only the installation wrapper as our unprivileged test user; its fake
# installer records argv instead of activating a release or touching services.
host_exec_sudo() { bash -lc "$3"; }
YELLOW='' NC=''
export OQTO_BOOTSTRAP_TEST_MARKER="$scratch/installer-argv"
name='oqto-v0.5.0-x86_64-unknown-linux-gnu'
mkdir -p "$scratch/stage/$name/immutable/bin" \
  "$scratch/stage/$name/immutable/frontend/dist" "$scratch/quoted'path"
printf 'manifest_version = 1\nid = "oqto-dist"\n[release]\ntarget = "full"\n' > "$scratch/stage/$name/manifest.toml"
printf '<main>Oqto</main>\n' > "$scratch/stage/$name/immutable/frontend/dist/index.html"
for bin in oqto oqtoctl oqto-setup oqto-runner oqto-files oqto-sandbox oqto-usermgr pi-bridge; do
    printf '#!/bin/sh\nexit 0\n' > "$scratch/stage/$name/immutable/bin/$bin"
    chmod 0755 "$scratch/stage/$name/immutable/bin/$bin"
done
printf '#!/bin/sh\nprintf "%%s\\n" "$@" > "$OQTO_BOOTSTRAP_TEST_MARKER"\n' > "$scratch/stage/$name/immutable/bin/oqto-setup"
chmod 0755 "$scratch/stage/$name/immutable/bin/oqto-setup"
DEPLOY_ARTIFACT="$scratch/quoted'path/$name.tar.gz"
DEPLOY_CHECKSUM="$scratch/quoted'path/$name.tar.gz.sha256"
tar -C "$scratch/stage" -czf "$DEPLOY_ARTIFACT" "$name"
(cd "$scratch/quoted'path" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
DRY_RUN=false
deploy_via_oqto_setup_install local-test '' true > "$scratch/output"
grep -Fxq -- 'install' "$OQTO_BOOTSTRAP_TEST_MARKER"
grep -Fxq -- '--checksum' "$OQTO_BOOTSTRAP_TEST_MARKER"
grep -Fxq -- '--artifact' "$OQTO_BOOTSTRAP_TEST_MARKER"
grep -q 'verified installer extracted' "$scratch/output"
# The same wrapper must never run its embedded binary if the archive's bytes
# differ from the separately supplied checksum.
rm -f "$OQTO_BOOTSTRAP_TEST_MARKER"
printf 'altered' >> "$DEPLOY_ARTIFACT"
if deploy_via_oqto_setup_install local-test '' true > "$scratch/bad-output" 2>&1; then
    echo 'modified release was executed' >&2; exit 1
fi
[[ ! -e "$OQTO_BOOTSTRAP_TEST_MARKER" ]]
echo 'verified deploy bootstrap wrapper tests passed'
