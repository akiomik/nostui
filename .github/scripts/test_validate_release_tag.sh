#!/usr/bin/env bash
set -euo pipefail

validator="$(dirname "$0")/validate-release-tag.sh"

for tag in v0.1.2 0.1.2 v10.20.30; do
  bash "$validator" "$tag"
done

for tag in v1draft 1backup v1.2 v1.2.3-beta v1.2.3.4; do
  if bash "$validator" "$tag" >/dev/null 2>&1; then
    echo "Accepted invalid release tag: $tag" >&2
    exit 1
  fi
done
