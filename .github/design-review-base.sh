#!/bin/sh
# The gate's static stage needs a revision preceding the change it checks.
# Keep this adapter with that caller; remove it if the gate no longer needs
# event-specific design review bases. Stdout contains only the resolved commit.
set -eu

if [ "$#" -ne 3 ]; then
  echo 'usage: design-review-base.sh EVENT REF PUSH_BEFORE' >&2
  exit 1
fi
event=$1
ref=$2
before=$3
case "$event" in
  push|workflow_dispatch|pull_request) ;;
  *) echo "unsupported design review event: $event" >&2; exit 1 ;;
esac

if [ "$ref" = refs/heads/main ]; then
  if [ "$event" = push ]; then
    candidate=$before
  else
    # A manual main run checks the last revision against its first parent.
    candidate=HEAD^
  fi
else
  # New main-side commits are not changes made by this work branch; for a
  # pull request HEAD is its merge commit, whose base is main's tip.
  candidate=$(git merge-base origin/main HEAD)
fi

if ! base=$(git rev-parse --verify --end-of-options "${candidate}^{commit}"); then
  echo "cannot resolve design review base: $candidate" >&2
  exit 1
fi
if [ "$ref" = refs/heads/main ] && [ "$base" = "$(git rev-parse HEAD)" ]; then
  echo 'main design review base equals HEAD; refusing an empty comparison' >&2
  exit 1
fi
printf '%s\n' "$base"
