#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"

repos=(
  "."
  "os"
  "user"
  "OSGuide"
  "testsuits-for-oskernel"
  "submit_repo"
  "vendor/smoltcp"
  "vendor/virtio-drivers"
  "exampleOs/arceos"
  "exampleOs/oskernel2025-rocketos"
  "exampleOs/starry-mix"
)

for repo in "${repos[@]}"; do
  dir="$ROOT/$repo"
  if [ "$repo" = "." ]; then
    git_dir="$ROOT/.git"
    label="root"
  else
    git_dir="$dir/.git"
    label="$repo"
  fi

  if [ ! -d "$git_dir" ]; then
    continue
  fi

  printf "\n[%s]\n" "$label"
  git -C "$dir" status --short --branch
done
