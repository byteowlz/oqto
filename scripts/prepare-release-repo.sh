#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Prepare a clean octo repo for public/alpha release
#
# What this does:
#   1. Clones the current repo (single branch: dev/a2ui) into a new directory
#   2. Rewrites all author/committer emails to GitHub noreply
#   3. Drops noise commits (trx sync, beads, chore: cleanup)
#   4. Collapses revert/reapply chains
#   5. Leaves you ready for interactive rebase to squash early history
#
# Usage:
#   ./scripts/prepare-release-repo.sh [target-dir]
#
# Default target: ../octo-release
# =============================================================================

SOURCE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="${1:-$(dirname "$SOURCE_DIR")/octo-release}"

NOREPLY_EMAIL="redacted@users.noreply.github.com"
AUTHOR_NAME="REDACTED"

echo "=== Octo Release Repo Preparation ==="
echo "Source:  $SOURCE_DIR"
echo "Target:  $TARGET_DIR"
echo "Author:  $AUTHOR_NAME <$NOREPLY_EMAIL>"
echo ""

# --- Safety checks ---
if [ -d "$TARGET_DIR" ]; then
    echo "ERROR: Target directory already exists: $TARGET_DIR"
    echo "Remove it first if you want to start fresh:"
    echo "  rm -rf $TARGET_DIR"
    exit 1
fi

# --- Step 1: Clone single branch ---
echo ">>> Step 1: Cloning dev/a2ui branch into $TARGET_DIR ..."
git clone \
    --single-branch \
    --branch dev/a2ui \
    --no-tags \
    "$SOURCE_DIR" \
    "$TARGET_DIR"

cd "$TARGET_DIR"

# Remove the origin pointing to the local repo (we'll set a new one later)
git remote remove origin

# Rename branch to main
git branch -m dev/a2ui main

echo "    Cloned $(git rev-list --count HEAD) commits on 'main'"

# --- Step 2: Rewrite authorship ---
echo ""
echo ">>> Step 2: Rewriting all author/committer identities ..."

MAILMAP_FILE=$(mktemp /tmp/octo-mailmap.XXXXXX)
cat > "$MAILMAP_FILE" <<EOF
$AUTHOR_NAME <$NOREPLY_EMAIL> <redacted@example.com>
$AUTHOR_NAME <$NOREPLY_EMAIL> <redacted@example.com>
$AUTHOR_NAME <$NOREPLY_EMAIL> <redacted@users.noreply.github.com>
$AUTHOR_NAME <$NOREPLY_EMAIL> <redacted@example.com>
$AUTHOR_NAME <$NOREPLY_EMAIL> REDACTED <redacted@example.com>
EOF

git filter-repo --mailmap "$MAILMAP_FILE" --force
rm -f "$MAILMAP_FILE"

# Verify
UNIQUE_EMAILS=$(git log --format="%ae" | sort -u)
echo "    Remaining emails after rewrite:"
echo "$UNIQUE_EMAILS" | sed 's/^/      /'

if echo "$UNIQUE_EMAILS" | grep -qv "$NOREPLY_EMAIL"; then
    echo "    WARNING: Found emails that weren't rewritten!"
else
    echo "    All emails unified to noreply address."
fi

# --- Step 3: Generate rebase plan ---
echo ""
echo ">>> Step 3: Generating interactive rebase instructions ..."

# Count what we'd drop
NOISE_COUNT=$(git log --oneline | grep -ciE "^[a-f0-9]+ (trx:|beads:)" || true)
CLEANUP_COUNT=$(git log --oneline | grep -ci "^[a-f0-9]+ chore: cleanup$" || true)
REVERT_COUNT=$(git log --oneline | grep -ci "^[a-f0-9]+ revert " || true)
REAPPLY_COUNT=$(git log --oneline | grep -ci "^[a-f0-9]+ reapply " || true)
TOTAL_COMMITS=$(git rev-list --count HEAD)

echo "    Total commits:       $TOTAL_COMMITS"
echo "    Noise (trx/beads):   $NOISE_COUNT (will be dropped)"
echo "    Cleanup commits:     $CLEANUP_COUNT (will be dropped)"
echo "    Reverts:             $REVERT_COUNT (collapse with reapplies)"
echo "    Reapplies:           $REAPPLY_COUNT"
echo ""

# Create a helper script for the interactive rebase
cat > "$TARGET_DIR/rebase-helper.sh" <<'REBASE_SCRIPT'
#!/usr/bin/env bash
set -euo pipefail

# This script generates a suggested rebase todo from the current git log.
# Run it, review the output, then use it with:
#   GIT_SEQUENCE_EDITOR="cp /path/to/suggested-todo" git rebase -i --root

echo "Generating suggested rebase todo..."

SUGGESTED="$PWD/suggested-rebase-todo.txt"
> "$SUGGESTED"

git log --reverse --format="%H %s" | while IFS=' ' read -r hash msg; do
    short=$(git rev-parse --short "$hash")

    # Drop noise commits
    if echo "$msg" | grep -qiE "^(trx:|beads:)"; then
        echo "drop $short $msg" >> "$SUGGESTED"
    elif echo "$msg" | grep -qi "^chore: cleanup$"; then
        echo "drop $short $msg" >> "$SUGGESTED"
    # Drop reverts (they'll be collapsed with reapplies)
    elif echo "$msg" | grep -qi "^Revert "; then
        echo "drop $short $msg" >> "$SUGGESTED"
    elif echo "$msg" | grep -qi "^Reapply "; then
        echo "drop $short $msg" >> "$SUGGESTED"
    # Squash early duplicate commits (same message patterns)
    elif echo "$msg" | grep -qi "^docs: updated beads"; then
        echo "drop $short $msg" >> "$SUGGESTED"
    elif echo "$msg" | grep -qi "^docs: added beads"; then
        echo "drop $short $msg" >> "$SUGGESTED"
    else
        echo "pick $short $msg" >> "$SUGGESTED"
    fi
done

TOTAL=$(wc -l < "$SUGGESTED")
PICKS=$(grep -c "^pick" "$SUGGESTED")
DROPS=$(grep -c "^drop" "$SUGGESTED")

echo ""
echo "Suggested todo written to: $SUGGESTED"
echo "  Total: $TOTAL  |  pick: $PICKS  |  drop: $DROPS"
echo ""
echo "Review and edit the file, then run:"
echo "  GIT_SEQUENCE_EDITOR='cp $SUGGESTED' git rebase -i --root --committer-date-is-author-date"
echo ""
echo "The --committer-date-is-author-date flag preserves the original timeline."
REBASE_SCRIPT
chmod +x "$TARGET_DIR/rebase-helper.sh"

# --- Done ---
echo "=== Done ==="
echo ""
echo "Your clean repo is at: $TARGET_DIR"
echo ""
echo "Next steps:"
echo "  1. cd $TARGET_DIR"
echo "  2. Review the history:  git log --oneline | less"
echo "  3. Run the rebase helper:  ./rebase-helper.sh"
echo "  4. Review suggested-rebase-todo.txt and adjust squash/drop decisions"
echo "  5. Apply:  GIT_SEQUENCE_EDITOR='cp ./suggested-rebase-todo.txt' git rebase -i --root --committer-date-is-author-date"
echo "  6. Verify:  git log --format='%ae %ad %s' --date=short | head -20"
echo "  7. Create new GitHub repo and push:"
echo "     git remote add origin git@github.com:byteowlz/octo.git"
echo "     git push -u origin main"
echo "     git tag -a v0.1.0-alpha -m 'Initial alpha release'"
echo "     git push --tags"
