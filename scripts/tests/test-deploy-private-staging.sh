#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT
python3 - "$ROOT_DIR/scripts/deploy.sh" "$scratch/stage-function.sh" <<'PY'
from pathlib import Path
import sys
source = Path(sys.argv[1]).read_text()
body = source.split("\nstage_private_remote_release() {\n", 1)[1].split("\n}\n", 1)[0]
Path(sys.argv[2]).write_text("stage_private_remote_release() {\n" + body + "\n}\n")
PY
mkdir -p "$scratch/fake-bin" "$scratch/quote'folder"
printf 'fixture\n' > "$scratch/quote'folder/oqto-v0.5.0-x86_64-unknown-linux-gnu.tar.gz"
printf 'hash placeholder\n' > "$scratch/quote'folder/archive.sha256"
cat > "$scratch/fake-bin/ssh" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$@" > "$FAKE_SSH_ARGS"
printf '%s\n' '/tmp/oqto-deploy.ABCD1234'
SH
cat > "$scratch/fake-bin/scp" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$@" > "$FAKE_SCP_ARGS"
SH
chmod 0755 "$scratch/fake-bin/ssh" "$scratch/fake-bin/scp"
export FAKE_SSH_ARGS="$scratch/ssh-args" FAKE_SCP_ARGS="$scratch/scp-args"
export PATH="$scratch/fake-bin:$PATH"
# Test the production staging function without running deploy's main entrypoint.
# This verifies argv boundaries on a path containing shell syntax and a
# per-host unpredictable private staging directory, without remote mutation.
source "$scratch/stage-function.sh"
err() { printf 'stage error: %s\n' "$*" >&2; }
DEPLOY_ARTIFACT="$scratch/quote'folder/oqto-v0.5.0-x86_64-unknown-linux-gnu.tar.gz"
DEPLOY_CHECKSUM="$scratch/quote'folder/archive.sha256"
remote_dir="$(stage_private_remote_release test-host)"
[[ "$remote_dir" == /tmp/oqto-deploy.ABCD1234 ]]
[[ "$(head -1 "$FAKE_SCP_ARGS")" == -- ]]
[[ "$(grep -Fx -- "$(realpath -e -- "$DEPLOY_ARTIFACT")" "$FAKE_SCP_ARGS")" == "$(realpath -e -- "$DEPLOY_ARTIFACT")" ]]
[[ "$(grep -Fx -- "$(realpath -e -- "$DEPLOY_CHECKSUM")" "$FAKE_SCP_ARGS")" == "$(realpath -e -- "$DEPLOY_CHECKSUM")" ]]
# A colon-bearing source must be denied before any remote command can run.
mkdir -p "$scratch/ambiguous:source"
DEPLOY_ARTIFACT="$scratch/ambiguous:source/oqto-v0.5.0-x86_64-unknown-linux-gnu.tar.gz"
cp -- "$scratch/quote'folder/oqto-v0.5.0-x86_64-unknown-linux-gnu.tar.gz" "$DEPLOY_ARTIFACT"
rm -f "$FAKE_SSH_ARGS" "$FAKE_SCP_ARGS"
if stage_private_remote_release test-host > /dev/null 2>&1; then
    echo 'ambiguous local scp source unexpectedly accepted' >&2; exit 1
fi
[[ ! -e "$FAKE_SSH_ARGS" && ! -e "$FAKE_SCP_ARGS" ]]
echo 'private deploy staging tests passed'
