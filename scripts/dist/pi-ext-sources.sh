#!/usr/bin/env bash
# shellcheck shell=bash
#
# Pinned-source resolution for pi-agent-extensions.
#
# Two repositories carry this project:
#   * the private working repository on Forgejo (original history, private paths),
#   * the public mirror on GitHub, whose history was rewritten: private files are
#     gone and sensitive strings replaced, so a public pin and its private
#     equivalent are DIFFERENT commit IDs for the same tree.
#
# Release/public builds must pin a commit that exists on the public mirror.
# Development builds may explicitly prefer a private URL through the environment;
# the default always works without private repository credentials.
#
# This file is sourced by scripts/dist/sync.sh and exercised by
# scripts/tests/test-dist-extension-sources.sh; it defines functions only.

PI_EXT_PUBLIC_URL_DEFAULT="https://github.com/byteowlz/pi-agent-extensions.git"

_pi_ext_log() { printf '[dist-sync] %s\n' "$*" >&2; }

# _pi_ext_fetch_url CACHE_DIR URL
# Bring a URL's refs into the cache. A commit that is not an advertised ref tip
# can still be requested directly on hosts that allow it, so try both.
_pi_ext_fetch_url() {
  local cache="$1" url="$2" ref="$3"
  git -C "$cache" fetch --quiet --no-tags "$url" "$ref" >/dev/null 2>&1 && return 0
  git -C "$cache" fetch --quiet --no-tags "$url" \
    '+refs/heads/*:refs/remotes/pinned/*' '+refs/tags/*:refs/tags/*' >/dev/null 2>&1
}

# _pi_ext_checkout CACHE REF
_pi_ext_checkout() {
  local cache="$1" ref="$2"
  git -C "$cache" checkout --quiet --detach "$ref" >/dev/null 2>&1 || return 1
  local actual expected
  actual="$(git -C "$cache" rev-parse HEAD)"
  expected="$(git -C "$cache" rev-parse "${ref}^{commit}")"
  [[ "$actual" == "$expected" ]]
}

# pi_ext_resolve_source CACHE_DIR LOCAL_PATH URLS REF NAME
#
# URLS is a space-separated preference list (private first, mirror second).
# Prints the checkout path to use and nothing else on stdout.
pi_ext_resolve_source() {
  local cache="$1" local_path="$2" urls="$3" ref="$4" name="$5"
  local -a candidates=()
  read -r -a candidates <<<"$urls"
  local primary="${candidates[0]}"

  # A sibling development checkout is used only when it is exactly the pinned,
  # clean commit. Anything else is left untouched.
  if [[ -e "$local_path" ]]; then
    local actual expected
    actual="$(git -C "$local_path" rev-parse HEAD 2>/dev/null || true)"
    expected="$(git -C "$local_path" rev-parse "${ref}^{commit}" 2>/dev/null || true)"
    if [[ -n "$actual" && "$actual" == "$expected" \
      && -z "$(git -C "$local_path" status --porcelain 2>/dev/null)" ]]; then
      printf '%s\n' "$local_path"
      return 0
    fi
    _pi_ext_log "$name at $local_path is not the clean pinned commit $ref; using a pinned clone"
  fi

  if ! command -v git >/dev/null 2>&1; then
    printf 'error: %s source missing at %s and git is not available to fetch %s\n' \
      "$name" "$local_path" "$primary" >&2
    return 1
  fi

  mkdir -p "$(dirname "$cache")"
  if [[ ! -d "$cache/.git" ]]; then
    [[ ! -e "$cache" ]] || {
      printf 'error: refusing to overwrite unknown %s cache at %s\n' "$name" "$cache" >&2
      return 1
    }
    _pi_ext_log "cloning $name from $primary into $cache"
    git clone --quiet --filter=blob:none "$primary" "$cache" >&2 || {
      printf 'error: cannot clone %s from %s\n' "$name" "$primary" >&2
      return 1
    }
    git -C "$cache" remote set-url origin "$primary" >/dev/null 2>&1 || true
  else
    _pi_ext_log "updating cached $name checkout $cache"
    _pi_ext_fetch_url "$cache" "$primary" "$ref" || true
  fi

  [[ -z "$(git -C "$cache" status --porcelain)" ]] || {
    printf 'error: refusing to switch dirty %s cache at %s\n' "$name" "$cache" >&2
    return 1
  }

  local url
  for url in "${candidates[@]}"; do
    [[ -n "$url" ]] || continue
    if _pi_ext_fetch_url "$cache" "$url" "$ref"; then
      if _pi_ext_checkout "$cache" "$ref"; then
        if [[ -z "$(git -C "$cache" status --porcelain)" ]]; then
          if [[ "$url" != "$primary" ]]; then
            _pi_ext_log "$name pin $ref is not in $primary; resolved it from $url (rewritten public equivalent)"
          fi
          printf '%s\n' "$cache"
          return 0
        fi
        printf 'error: cached %s is dirty after checkout of %s\n' "$name" "$ref" >&2
        return 1
      fi
    fi
  done

  printf 'error: %s pin %s exists in none of: %s\n' "$name" "$ref" "${candidates[*]}" >&2
  printf 'hint: public pins must exist on the mirror; private rewritten pairs are listed in\n' >&2
  printf '      ~/wiki/guides/private-working-repos-public-mirrors.pi-agent-extensions-commit-map.txt\n' >&2
  return 1
}
