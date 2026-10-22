#!/usr/bin/env bash
set -euo pipefail

tag=${1:?expected a release tag}
if [[ $tag =~ ^v?[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  exit 0
fi

echo "Invalid release tag '$tag': expected vX.Y.Z or X.Y.Z" >&2
exit 1
