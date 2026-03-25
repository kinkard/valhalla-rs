#!/usr/bin/env bash
#
# Generates a commit message for valhalla submodule updates.
#
# Usage:
#   ./scripts/update-valhalla-commit-msg.sh
#
# Prerequisites: the valhalla submodule must have a pending change
# (i.e. `git diff valhalla` shows a submodule update).

set -euo pipefail

SUBMODULE_PATH="valhalla"
REMOTE_URL="https://github.com/valhalla/valhalla"

# Get the old (committed) and new (working tree) submodule commits
old_commit=$(git diff "$SUBMODULE_PATH" | grep '^-Subproject commit' | awk '{print $3}')
new_commit=$(git diff "$SUBMODULE_PATH" | grep '^\+Subproject commit' | awk '{print $3}')

if [ -z "$old_commit" ] || [ -z "$new_commit" ]; then
    echo "Error: No pending submodule change detected for '$SUBMODULE_PATH'." >&2
    echo "Make sure the valhalla submodule has been updated but not yet committed." >&2
    exit 1
fi

# Build the compare URL
compare_url="${REMOTE_URL}/compare/${old_commit}..${new_commit}"

# Get first-parent commit subjects between old and new
changelog=$(git -C "$SUBMODULE_PATH" log --first-parent --format='%s' "${old_commit}..${new_commit}" \
    | sed 's/^/- /')

cat <<EOF
chore: Update valhalla

${compare_url}

${changelog}
EOF
