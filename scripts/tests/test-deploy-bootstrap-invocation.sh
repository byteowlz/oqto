#!/usr/bin/env bash
# The deploy driver runs exactly one target transaction whose installer is
# extracted from the checksum-verified artifact, never taken from PATH, and
# never runs anything when the artifact bytes differ from the checksum.
# Functions are extracted from deploy.sh at runtime and read these globals.
# shellcheck disable=SC1091,SC2034
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT
python3 - "$ROOT_DIR/scripts/deploy.sh" "$scratch/functions.sh" <<'PY'
from pathlib import Path
import sys
text = Path(sys.argv[1]).read_text()
out = []
for name in ("profile_for", "stage_for_host", "transaction_args", "run_transaction"):
    body = text.split(f"\n{name}() {{\n", 1)[1].split("\n}\n", 1)[0]
    out.append(f"{name}() {{\n{body}\n}}\n")
Path(sys.argv[2]).write_text("\n".join(out))
PY
source "$scratch/functions.sh"
err() { printf 'deploy error: %s\n' "$*" >&2; }
# Execute the target script as the unprivileged test user; the fake
# transaction records argv instead of activating or touching services.
host_exec_sudo() { bash -c "$3"; }
cleanup_private_remote_release() { :; }

name='oqto-0.5.0-x86_64-unknown-linux-gnu'
mkdir -p "$scratch/stage/$name/immutable/bin" "$scratch/stage/$name/immutable/frontend" "$scratch/quoted'path"
printf 'manifest_version = 1\nid = "oqto-dist"\n[release]\ntarget = "full"\n' > "$scratch/stage/$name/manifest.toml"
printf '<main>Oqto</main>\n' > "$scratch/stage/$name/immutable/frontend/index.html"
for bin in oqto oqtoctl oqto-setup oqto-runner oqto-files oqto-sandbox oqto-ssh-proxy oqto-usermgr pi-bridge; do
    printf '#!/bin/sh\nexit 0\n' > "$scratch/stage/$name/immutable/bin/$bin"
    chmod 0755 "$scratch/stage/$name/immutable/bin/$bin"
done
printf '#!/bin/sh\necho artifact-installer\n' > "$scratch/stage/$name/immutable/bin/oqto-setup"
DEPLOY_ARTIFACT="$scratch/quoted'path/$name.tar.gz"
DEPLOY_CHECKSUM="$scratch/quoted'path/$name.tar.gz.sha256"
tar -C "$scratch/stage" -czf "$DEPLOY_ARTIFACT" "$name"
(cd "$scratch/quoted'path" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")

cat > "$scratch/fake-transaction.py" <<'PY'
import os, subprocess, sys
args = sys.argv[1:]
with open(os.environ["MARKER"], "w") as out:
    out.write("\n".join(args) + "\n")
    if "--setup" in args:
        setup = args[args.index("--setup") + 1]
        out.write("installer-says=" + subprocess.run([setup], capture_output=True, text=True).stdout)
PY
TRANSACTION="$scratch/fake-transaction.py"
BOOTSTRAP="$ROOT_DIR/scripts/dist/verified-setup-bootstrap.py"
export MARKER="$scratch/argv"
H_NAME=(local-test) H_SSH=('') H_MODE=(single-user) H_USER=(alice) H_LOCAL=(true)
DRY_RUN=false ALLOW_BUSY_SESSIONS=false ALLOW_DOWNGRADE=true ALLOW_DIRTY_SOURCE=false
HEALTH_TIMEOUT_SECONDS=42 KEEP_RELEASES=2 BACKFILL_MODES=validate BACKFILL_PRINCIPALS=()

run_transaction 0 activate
grep -Fxq activate "$MARKER"
grep -Fxq -- '--allow-downgrade' "$MARKER"
grep -Fxq -- '--principal' "$MARKER" && grep -Fxq alice "$MARKER"
grep -Fxq -- '--health-budget' "$MARKER" && grep -Fxq 42 "$MARKER"
grep -Fxq 'installer-says=artifact-installer' "$MARKER"
if grep -Fxq -- '--allow-dirty-source' "$MARKER"; then echo 'unrequested override forwarded' >&2; exit 1; fi

rm -f "$MARKER"
run_transaction 0 backfill
grep -Fxq backfill "$MARKER"
grep -Fxq validate "$MARKER"
if grep -Fxq -- '--setup' "$MARKER"; then echo 'backfill extracted an installer' >&2; exit 1; fi

# Altered bytes: the bootstrap refuses and the transaction never starts.
rm -f "$MARKER"
printf 'altered' >> "$DEPLOY_ARTIFACT"
if run_transaction 0 activate > "$scratch/bad-output" 2>&1; then
    echo 'modified release was activated' >&2; exit 1
fi
[[ ! -e "$MARKER" ]]
echo 'verified deploy transaction invocation tests passed'
