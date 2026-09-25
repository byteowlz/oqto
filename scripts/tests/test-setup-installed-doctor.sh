#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT
mkdir -p "$scratch/bin"

cat > "$scratch/bin/cargo" <<'SH'
#!/usr/bin/env bash
echo 'ERROR: read-only setup used source cargo on an installed host' >&2
exit 88
SH
cat > "$scratch/bin/oqtoctl" <<'SH'
#!/usr/bin/env bash
if [[ "$*" == 'doctor --help' ]]; then
  printf '%s\n' '--contract'
  exit 0
fi
printf 'oqtoctl %s\n' "$*" >> "$STUB_LOG"
SH
cat > "$scratch/bin/oqto-setup" <<'SH'
#!/usr/bin/env bash
if [[ "$*" == '--help' ]]; then
  printf '%s\n' 'plan'
  exit 0
fi
printf 'oqto-setup %s\n' "$*" >> "$STUB_LOG"
SH
chmod +x "$scratch/bin/"*

export PATH="$scratch/bin:$PATH" STUB_LOG="$scratch/calls"
(cd "$root" && ./setup.sh --personal --doctor --strict > "$scratch/doctor-output" 2>&1) || {
  echo 'installed doctor was not used on the source checkout:' >&2
  tail -12 "$scratch/doctor-output" >&2
  exit 1
}
grep -Fxq 'oqtoctl doctor --contract --profile personal --strict' "$scratch/calls"
(cd "$root" && ./setup.sh --personal --plan > "$scratch/plan-output" 2>&1) || {
  echo 'installed plan was not used on the source checkout:' >&2
  tail -12 "$scratch/plan-output" >&2
  exit 1
}
grep -Fxq 'oqto-setup plan --profile personal' "$scratch/calls"

cat > "$scratch/bin/oqtoctl" <<'SH'
#!/usr/bin/env bash
if [[ "$*" == 'doctor --help' ]]; then
  printf '%s\n' 'legacy doctor without contract'
  exit 0
fi
exit 88
SH
chmod +x "$scratch/bin/oqtoctl"
if (cd "$root" && ./setup.sh --personal --doctor > "$scratch/incompatible-output" 2>&1); then
  echo 'incompatible installed doctor was accepted' >&2
  exit 1
fi
grep -Fq 'installed oqtoctl does not support --contract' "$scratch/incompatible-output" || {
  echo 'incompatible installed doctor silently fell back to a source build' >&2
  tail -10 "$scratch/incompatible-output" >&2
  exit 1
}

echo 'setup read-only entrypoints prefer installed tools and reject incompatible binaries without source builds'
