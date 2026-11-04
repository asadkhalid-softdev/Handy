#!/usr/bin/env bash
set -euo pipefail

owner=asadkhalid-softdev
fork="$owner/Handy"
branch=$(git branch --show-current)
if [[ "$branch" != main && "$branch" != feat/windows-wake-word-control ]]; then
  echo "Switch to main or feat/windows-wake-word-control before publishing." >&2
  exit 1
fi
if [[ $(gh api user --jq .login) != "$owner" ]]; then
  echo "Sign in to GitHub as $owner before creating this fork." >&2
  exit 1
fi
if ! gh repo view "$fork" --json nameWithOwner >/dev/null 2>&1; then
  gh repo fork cjpais/Handy --clone=false --remote=false
fi
parent=$(gh repo view "$fork" --json parent --jq .parent.nameWithOwner)
if [[ "$parent" != cjpais/Handy ]]; then
  echo "$fork is not a fork of cjpais/Handy; remotes were not changed." >&2
  exit 1
fi
if git remote get-url upstream >/dev/null 2>&1; then
  upstream=$(git remote get-url upstream)
  if [[ "$upstream" != *cjpais/Handy* ]]; then
    echo "An unrelated upstream remote already exists; leaving it unchanged." >&2
    exit 1
  fi
else
  git remote add upstream https://github.com/cjpais/Handy.git
fi
git remote set-url origin "https://github.com/$fork.git"
git -c credential.helper= -c 'credential.helper=!gh auth git-credential' push --set-upstream origin "$branch"
echo "Published: https://github.com/$fork/tree/$branch"
