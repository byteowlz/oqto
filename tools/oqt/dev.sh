#!/usr/bin/env bash
# Developer source launcher. Installed oqt uses an independent runtime snapshot.
set -euo pipefail
oqt_t0=$EPOCHREALTIME
# Resolve source aliases, then locate the repository from this component.
self=$(readlink -f "${BASH_SOURCE[0]}")
root=$(cd "$(dirname "$self")/../.." && pwd)
# Explicit qualified source only; never select an ambient /tmp checkout.
source=${OQT_PI_SOURCE:-$HOME/.local/share/oqt/pi-source}
workdir=$PWD
mode=coding
owner=${OQT_OWNER_AGENT_DIR:-${PI_CODING_AGENT_DIR:-$HOME/.pi/agent}}
args=()
# Explicit inspection/discovery is independent of Pi, profiles and build caches.
if [[ "${1-}" == inspect-classic || "${1-}" == list-classic ]]; then
 exec node "$root/tools/oqt/classic-inspect.ts" "$@"
fi
while (($#)); do
 case "$1" in
  --workdir) workdir=${2:?--workdir needs a directory}; shift 2;;
  --coding) args+=("$1"); shift;;
  --owner-profile) owner=${2:?--owner-profile needs a directory}; shift 2;;
  --demo) mode=demo; args+=("$1"); shift;;
  --continue|-c|--smoke|--json) args+=("$1"); shift;;
  --import-classic) file=${2:?--import-classic needs a file}; [[ "$file" == /* ]] || file="$PWD/$file"; args+=("$1" "$file"); shift 2;;
  --origin|--leaf) args+=("$1" "${2:?option needs a value}"); shift 2;;
  -h|--help) printf '%s\n' 'oqt: experimental Pi Durable workspace coding' 'Usage: oqt [--continue] [--workdir DIR] [--owner-profile DIR]' 'Import: oqt --import-classic FILE --origin NAME [--leaf ID] [--json] [--continue] --workdir DIR' 'Read-only: oqt inspect-classic FILE --origin NAME --json | oqt list-classic DIR --origin NAME --json' 'Default: real coding. Owning Pi defaults; credentials brokered by a guard (never copied). Confined brain and hands.' 'Every tool call needs approval; request_access asks for outside paths/network (Session, 1h).' 'Ceilings: ~/.config/oqto/sandbox.toml and <workspace>/.oqto/sandbox.toml (narrow only). --continue joins/reopens. --demo = synthetic fixture demo.' 'Local OS operator boundary, not Oqto Account authentication. --demo keeps the synthetic demo. Nothing is installed.'; exit 0;;
  *) echo "Unsupported oqt prototype option: $1" >&2; exit 2;;
 esac
done
case "$(uname -s)/$(uname -m)" in
 Linux/x86_64) platform=linux;;
 Darwin/arm64) platform=darwin;;
 *) echo 'Prototype requires Linux x86_64 or macOS arm64' >&2; exit 2;;
esac
workdir=$(cd "$workdir" 2>/dev/null && pwd) || { echo "Work directory does not exist: $workdir" >&2; exit 2; }
if [[ "$mode" == coding ]]; then
 case "$workdir" in
  "$root"|"$root"/*) echo "Refusing: $workdir is inside oqt's own source tree, which coding workspaces may never be (the agent could reach oqt's own approval machinery). Run from your project directory or pass --workdir /path/to/project." >&2; exit 2;;
 esac
fi
if [[ "$platform" == linux ]]; then
 for command in node python3 cargo bwrap taskset nice timeout; do command -v "$command" >/dev/null || { echo "Missing required command: $command" >&2; exit 2; }; done
 pinning=(taskset -c "$(python3 -c 'import os; print(min(os.sched_getaffinity(0)))')")
else
 # macOS: Seatbelt (sandbox-exec), no CPU pinning (no taskset), no seccomp.
 for command in python3 cargo nice timeout; do command -v "$command" >/dev/null || { echo "Missing required command: $command (macOS: brew install coreutils for timeout)" >&2; exit 2; }; done
 [[ -x /usr/bin/sandbox-exec ]] || { echo 'Missing /usr/bin/sandbox-exec; refusing unconfined execution' >&2; exit 2; }
 pinning=()
fi
# The confined brain runs this node. On macOS it must be self-contained (an
# official nodejs.org build): a Homebrew node loads dylibs from /opt/homebrew,
# which the brain's Seatbelt profile does not and should not expose.
node=${OQT_NODE:-$(command -v node || true)}
[[ -n "$node" && -x "$node" ]] || { echo 'Missing node (set OQT_NODE to a Node 26 binary)' >&2; exit 2; }
if [[ "$platform" == darwin ]] && otool -L "$node" | tail -n +2 | awk '{print $1}' | grep -Ev '^(/usr/lib/|/System/)' >/dev/null; then
 echo "Refusing node $node: it links libraries outside /usr/lib and /System. Set OQT_NODE to an official nodejs.org Node 26 build." >&2; exit 2
fi
export CARGO_BUILD_JOBS=1
# Runtime artifacts are local to this worktree, never installed globally.
# OQT_TARGET_DIR selects another target directory (for example a private one
# in a shared checkout).
export OQT_TARGET_DIR="${OQT_TARGET_DIR:-$root/backend/target}" CARGO_TARGET_DIR="${OQT_TARGET_DIR:-$root/backend/target}"
bounded() { timeout --kill-after=5s 90s "${pinning[@]}" nice -n 19 "$@"; }
cd "$root"
bpf=$(timeout --kill-after=5s 600s "${pinning[@]}" nice -n 19 python3 tools/oqt/build-assets.py "$source")
if [[ "$mode" == coding ]]; then
 owner=$(python3 -c 'from pathlib import Path; import sys; print(Path(sys.argv[1]).expanduser().resolve(strict=True))' "$owner")
fi
runtime=$(bounded python3 tools/oqt/prepare.py "$source" "$workdir" "$mode" "$owner")
# One parse: prepare.py's JSON -> an associative array (keys are fixed, values are paths).
declare -A rt
while IFS=$'\t' read -r key val; do rt[$key]=$val; done < <(python3 -c 'import json,sys
for k, v in json.loads(sys.argv[1]).items(): print(f"{k}\t{v}")' "$runtime")
value() { printf '%s' "${rt[$1]:?missing runtime key $1}"; }
# Linux confines with a compiled seccomp filter; macOS has none to pass.
seccomp=()
if [[ "$platform" == linux ]]; then seccomp=(OQT_SECCOMP_BPF="$(value bpf)"); fi
snapshot=$(value source); profile=$(value profile); home=$(value home); agent=$(value agent)
# Remote hands (ADR-0086): the owner's link config and private link state,
# resolved with the real HOME before the clean environment below.
link_config=${OQT_HANDS_LINK_CONFIG:-${XDG_CONFIG_HOME:-$HOME/.config}/oqt/hands-link.toml}
link_state=${OQT_HANDS_LINK_STATE:-${XDG_STATE_HOME:-$HOME/.local/state}/oqt/hands-link}
cd "$workdir"
# The interactive runtime runs at normal priority on all CPUs; only builds
# above are bounded (one CPU, nice 19).
exec env -i PATH="$PATH" HOME="$home" LANG="${LANG:-C.UTF-8}" \
 TERM="${TERM:-xterm-256color}" COLORTERM="${COLORTERM:-}" \
 NODE_OPTIONS=--max-old-space-size=512 PI_EXPERIMENTAL=1 PI_CODING_AGENT_DIR="$agent" \
 OQT_MODE="$mode" OQT_OWNER_AGENT_DIR="$owner" OQT_RUNTIME_ROOT="$(value runtime_root)" \
 OQT_TRUSTED_SOURCE_ROOT="$root" OQT_SOURCE_CHECKOUT="$source" \
 OQT_SERVER_DIR="$profile" OQT_CARRIER="$(value carrier)" \
 OQT_HANDS="$(value hands)" OQT_LINK="$(value link)" OQT_HANDS_LINK_CONFIG="$link_config" OQT_HANDS_LINK_STATE="$link_state" OQT_BRAIN="$(value brain)" PI_OFFLINE=1 OQT_ENV_DAEMON="$(value daemon)" OQT_SANDBOX_SHIM="$(value shim)" \
 "${seccomp[@]}" OQT_DEBUG="${OQT_DEBUG:-}" OQT_HOST_IDLE_SECONDS="${OQT_HOST_IDLE_SECONDS:-}" PI_PROGRAM_STATUS="${PI_PROGRAM_STATUS:-}" OQT_SANDBOX_CONFIG="${OQT_SANDBOX_CONFIG:-$HOME/.config/oqto/sandbox.toml}" OQT_INPUT_TRACE="$profile/input-trace.log" OQT_STARTUP_TRACE="$profile/startup-trace.log" OQT_SHELL_START="$oqt_t0" NODE_COMPILE_CACHE="$(value runtime_root)/node-compile-cache" \
 "$node" --import "$snapshot/packages/coding-agent/src/experimental/source-resolver.ts" \
 "$snapshot/packages/coding-agent/src/experimental/oqt-launcher.ts" "${args[@]}"
