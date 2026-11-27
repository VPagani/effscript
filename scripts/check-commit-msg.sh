#!/usr/bin/env bash
# Checks commit messages against the convention in CONTRIBUTING.md.
#
#   scripts/check-commit-msg.sh FILE          check the message in FILE (commit-msg hook)
#   scripts/check-commit-msg.sh --range A..B  check every commit in a range (CI)
set -euo pipefail

TYPES="feat|fix|perf|refactor|docs|test|build|ci|chore|revert"
SCOPES="lang|syntax|codegen|check|fmt|lint|lsp|mapper|cli|wasm|plugins|editor|npm|examples|docs|ci|deps"
HEADER="^(${TYPES})(\((${SCOPES})(,(${SCOPES}))*\))?!?: [^ ].*[^.]$"

check() {
  local subject="$1" where="$2"
  # Messages git writes itself.
  if [[ "$subject" =~ ^(Merge|Revert\ \"|fixup!|squash!|amend!) ]]; then
    return 0
  fi
  if ! [[ "$subject" =~ $HEADER ]]; then
    echo "error: $where: \"$subject\"" >&2
    echo "  expected <type>(<scope>): <description>, for example \"fix(codegen): keep spans for merged imports\"" >&2
    echo "  types: ${TYPES//|/, }" >&2
    echo "  scopes: ${SCOPES//|/, }" >&2
    return 1
  fi
  if (( ${#subject} > 72 )); then
    echo "error: $where: the subject line has ${#subject} characters; keep it to 72" >&2
    return 1
  fi
}

if [[ "${1:-}" == "--range" ]]; then
  status=0
  while IFS= read -r line; do
    sha="${line%% *}"
    check "${line#* }" "commit ${sha:0:7}" || status=1
  done < <(git log --format='%H %s' "$2")
  exit $status
fi

subject="$(grep -v '^#' "$1" | head -n 1)"
check "$subject" "commit message"
