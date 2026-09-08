#!/usr/bin/env bash
# Real Pi, real PTY, disposable HOME, no extensions/models/network/session writes.
set -euo pipefail
[[ "$(uname -s)" == Darwin ]] || { echo 'This proof requires macOS.' >&2; exit 1; }
export PATH="/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin"
root_dir="$(cd "$(dirname "$0")/../../.." && pwd)"
pi="${PI_BIN:-$HOME/.bun/bin/pi}"
package_root="${PI_PACKAGE_ROOT:-$HOME/.bun}"
sandbox="${OQTO_SANDBOX_BIN:-$root_dir/backend/target/debug/oqto-sandbox}"
[[ -x "$pi" && -x "$sandbox" ]] || { echo 'Pi/sandbox executable missing' >&2; exit 1; }
command -v tmux > /dev/null
fixture="$(mktemp -d /tmp/oq-tui-XXXXXX)"
cleanup() { tmux -S "$fixture/tmux.sock" kill-server 2>/dev/null || true; rm -rf "$fixture"; }
trap cleanup EXIT
mkdir -p "$fixture/home/.pi/agent" "$fixture/work"
printf private > "$fixture/work/denied"
printf 'profile = "minimal"\nno_new_privs = false\nread_policy = "allowlist"\nisolate_network = true\nallow_write = ["~/.pi"]\nextra_ro_bind = ["%s", "/opt/homebrew"]\ndeny_read = ["%s", "%s"]\n' \
  "$package_root" "$fixture/tmux.sock" "$fixture/work/denied" > "$fixture/sandbox.toml"
printf '#!/bin/bash\nexec ' > "$fixture/launch.sh"
printf '%q ' /usr/bin/env -i "HOME=$fixture/home" "PATH=$PATH" "TERM=xterm-256color" "TMPDIR=$fixture/work" \
  "PI_CODING_AGENT_DIR=$fixture/home/.pi/agent" "PI_OFFLINE=1" "PI_TELEMETRY=0" \
  "$sandbox" --config "$fixture/sandbox.toml" --workspace "$fixture/work" -- "$pi" \
  --no-session --no-extensions --no-skills --no-prompt-templates --no-themes --no-context-files --no-approve >> "$fixture/launch.sh"
printf '\n' >> "$fixture/launch.sh"
# Keep the pane briefly after an early Pi exit so failures have useful evidence.
tmux -S "$fixture/tmux.sock" -f /dev/null new-session -d -s proof -x 110 -y 32 \
  "/bin/bash $fixture/launch.sh; echo PI_EXIT=\$?; sleep 25"
sleep 5
tmux -S "$fixture/tmux.sock" send-keys -t proof -l \
  '!!printf TUI_OK > native-proof; if cat denied; then printf LEAK >> native-proof; else printf DENIED >> native-proof; fi'
tmux -S "$fixture/tmux.sock" send-keys -t proof Enter
for _ in {1..100}; do
  if [[ -f "$fixture/work/native-proof" ]] && grep -q DENIED "$fixture/work/native-proof"; then break; fi
  sleep 0.1
done
tmux -S "$fixture/tmux.sock" capture-pane -p -t proof > "$fixture/capture.txt"
if [[ ! -f "$fixture/work/native-proof" ]]; then
  cat "$fixture/capture.txt"; echo 'FAIL: Pi never wrote native-proof' >&2; exit 1
fi
observed="$(/bin/cat "$fixture/work/native-proof")"
if [[ "$observed" != TUI_OKDENIED ]]; then
  cat "$fixture/capture.txt"; echo 'FAIL: native shell escaped file policy' >&2; exit 1
fi
printf 'PASS: Pi native TUI raw mode, shell workdir write, denied read; no model call or real user config\n'
