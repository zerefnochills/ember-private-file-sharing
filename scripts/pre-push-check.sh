#!/usr/bin/env bash
# Quick safety check before pushing. Usage: scripts/pre-push-check.sh [--full]
# --full also runs scripts/check.sh (the whole Rust gate).
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
cd "$(git rev-parse --show-toplevel)" || exit 1

fail=0
bad()  { echo "FAIL: $*"; fail=1; }
warn() { echo "WARN: $*"; }

echo "== Branch =="
branch="$(git branch --show-current)"
echo "On branch: ${branch:-DETACHED HEAD}"
upstream="$(git rev-parse --abbrev-ref '@{u}' 2>/dev/null || true)"
if [ -n "$upstream" ]; then
  echo "Commits not yet pushed to $upstream:"
  git log --oneline "$upstream..HEAD"
else
  echo "No upstream yet: first push needs 'git push -u origin ${branch}'"
fi

echo "== Uncommitted changes =="
if [ -n "$(git status --porcelain)" ]; then
  warn "working tree is not clean:"
  git status --short
else
  echo "clean"
fi

echo "== Secret-like or local files (tracked or untracked and not ignored) =="
pattern='(^|/)\.env($|\.)|\.(pem|key|jks|keystore|p12)$|(^|/)local\.properties$|\.salive$'
found="$( { git ls-files; git ls-files --others --exclude-standard; } | sort -u \
          | grep -E "$pattern" | grep -v -E '\.env\.example$' || true )"
if [ -n "$found" ]; then bad "these files should not be committed:"; echo "$found"; else echo "none"; fi

echo "== Large files (over 5 MB) =="
large=""
while IFS= read -r f; do
  [ -f "$f" ] || continue
  size="$(wc -c < "$f")"
  if [ "$size" -gt 5242880 ]; then large="$large$f ($size bytes)"$'\n'; fi
done < <( { git ls-files; git ls-files --others --exclude-standard; } | sort -u )
if [ -n "$large" ]; then warn "large files:"; printf '%s' "$large"; else echo "none"; fi

echo "== Merge conflict markers =="
if git grep -n -E '^(<<<<<<< |>>>>>>> )' -- . ':!*.md' >/dev/null 2>&1; then
  bad "conflict markers found:"; git grep -n -E '^(<<<<<<< |>>>>>>> )' -- . ':!*.md'
else
  echo "none"
fi

echo "== Banned marketing words =="
if [ -f scripts/check_banned_words.sh ]; then
  bash scripts/check_banned_words.sh >/dev/null 2>&1 && echo "clean" || bad "banned words found (run scripts/check_banned_words.sh)"
else
  warn "scripts/check_banned_words.sh not found"
fi

if [ "${1:-}" = "--full" ]; then
  echo "== Full Rust gate =="
  bash scripts/check.sh || bad "scripts/check.sh failed"
fi

echo
if [ "$fail" -eq 0 ]; then echo "READY TO PUSH"; else echo "NOT READY: fix the FAIL lines above"; fi
exit "$fail"
