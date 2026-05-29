#!/usr/bin/env bash

# Copyright 2025 TiDB Cloud
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${DIST_DIR:-${ROOT_DIR}/dist/linux-packages}"
VERSION="${VERSION:?VERSION must be set}"
SEMVER_VERSION="${VERSION#v}"

debian_verify() {
  docker run --rm -v "${DIST_DIR}:/repo:ro" debian:bookworm-slim sh -euxc "
    cp /repo/keys/lakesql-archive-keyring.gpg /usr/share/keyrings/lakesql-archive-keyring.gpg
    echo 'deb [signed-by=/usr/share/keyrings/lakesql-archive-keyring.gpg] file:///repo/apt stable main' > /etc/apt/sources.list.d/lakesql.list
    apt-get update
    apt-get install -y lakesql
    lakesql --version | grep -F '${SEMVER_VERSION}'
  "
}

fedora_verify() {
  docker run --rm -v "${DIST_DIR}:/repo:ro" fedora:42 sh -euxc "
    cat > /etc/yum.repos.d/lakesql.repo <<'EOF'
[lakesql]
name=LakeSQL
baseurl=file:///repo/rpm/stable/x86_64
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey=file:///repo/keys/RPM-GPG-KEY-lakesql
EOF
    dnf install -y lakesql
    lakesql --version | grep -F '${SEMVER_VERSION}'
  "
}

alpine_verify() {
  docker run --rm -v "${DIST_DIR}:/repo:ro" alpine:3.22 sh -euxc "
    cp /repo/keys/lakesql-packages.rsa.pub /etc/apk/keys/lakesql-packages.rsa.pub
    printf '%s\n' 'file:///repo/apk/stable' > /etc/apk/repositories
    apk update
    apk add lakesql
    lakesql --version | grep -F '${SEMVER_VERSION}'
  "
}

debian_verify
fedora_verify
alpine_verify
