#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BASE_SHA="${1:-}"
HEAD_SHA="${2:-HEAD}"

DOC_FILE="docs/CHECK-JSON-CONTRACT.md"
TRIGGERS_FILE="${KX_CHECK_JSON_CONTRACT_DOCS_SYNC_TRIGGERS_FILE:-$ROOT/scripts/check_json_contract_docs_sync.triggers}"

load_triggers() {
  local trigger_file="$1"
  if [[ ! -f "$trigger_file" ]]; then
    echo "missing trigger file: $trigger_file" >&2
    exit 2
  fi

  mapfile -t TRIGGERS < <(sed -e 's/#.*$//' -e 's/[[:space:]]*$//' "$trigger_file" | sed '/^$/d')
  if ((${#TRIGGERS[@]} == 0)); then
    echo "no triggers found in: $trigger_file" >&2
    exit 2
  fi
}

ensure_commit() {
  local sha="$1"
  if [[ -z "$sha" ]]; then
    return 0
  fi
  if git cat-file -e "${sha}^{commit}" >/dev/null 2>&1; then
    return 0
  fi
  git fetch --no-tags --depth=1 origin "$sha" >/dev/null 2>&1 || true
  git cat-file -e "${sha}^{commit}" >/dev/null 2>&1
}

resolve_range() {
  if [[ -n "$BASE_SHA" ]]; then
    ensure_commit "$BASE_SHA" || {
      echo "cannot resolve base commit: $BASE_SHA" >&2
      exit 2
    }
    ensure_commit "$HEAD_SHA" || {
      echo "cannot resolve head commit: $HEAD_SHA" >&2
      exit 2
    }
    printf '%s..%s\n' "$BASE_SHA" "$HEAD_SHA"
    return
  fi

  if git rev-parse --verify HEAD^1 >/dev/null 2>&1; then
    printf '%s\n' "HEAD^1..HEAD"
    return
  fi

  echo "cannot determine diff range; pass base/head sha explicitly" >&2
  exit 2
}

diff_range="$(resolve_range)"
load_triggers "$TRIGGERS_FILE"
mapfile -t changed_files < <(git diff --name-only "$diff_range" | sed '/^$/d')

if ((${#changed_files[@]} == 0)); then
  echo "check-json-contract-docs-sync: no changed files in range $diff_range"
  exit 0
fi

docs_changed=0
contract_triggered=0
trigger_hits=()

for file in "${changed_files[@]}"; do
  if [[ "$file" == "$DOC_FILE" ]]; then
    docs_changed=1
  fi
  for trigger in "${TRIGGERS[@]}"; do
    if [[ "$file" == "$trigger" ]]; then
      contract_triggered=1
      trigger_hits+=("$file")
      break
    fi
  done
done

if (( contract_triggered == 1 && docs_changed == 0 )); then
  echo "check-json-contract-docs-sync: failed" >&2
  echo "contract-related files changed without $DOC_FILE update:" >&2
  printf '  - %s\n' "${trigger_hits[@]}" >&2
  exit 1
fi

if (( contract_triggered == 1 )); then
  echo "check-json-contract-docs-sync: pass (contract changes + docs synced)"
else
  echo "check-json-contract-docs-sync: pass (no contract trigger files changed)"
fi
