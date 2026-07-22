#!/usr/bin/env bash
set -euo pipefail

assets_dir="${1:?usage: publish-ghcr-images.sh <assets-dir> <repository-owner> <tag>}"
repository_owner="${2:?usage: publish-ghcr-images.sh <assets-dir> <repository-owner> <tag>}"
tag="${3:?usage: publish-ghcr-images.sh <assets-dir> <repository-owner> <tag>}"

image_archive="${assets_dir}/codexmanager-docker-images.tar.gz"
test -f "$image_archive" || {
  echo "docker image archive not found: $image_archive"
  exit 1
}

gunzip -c "$image_archive" | docker load

owner="$(printf '%s' "$repository_owner" | tr '[:upper:]' '[:lower:]')"

docker tag codexmanager-service:release "ghcr.io/${owner}/codexmanager-service:${tag}"
docker tag codexmanager-web:release "ghcr.io/${owner}/codexmanager-web:${tag}"

docker push "ghcr.io/${owner}/codexmanager-service:${tag}"
docker push "ghcr.io/${owner}/codexmanager-web:${tag}"
