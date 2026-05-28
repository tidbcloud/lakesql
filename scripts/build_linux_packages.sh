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
DIST_DIR="${DIST_DIR:-${ROOT_DIR}/dist}"
OUTPUT_DIR="${OUTPUT_DIR:-${DIST_DIR}/linux-packages}"
VERSION="${VERSION:?VERSION must be set}"
SEMVER_VERSION="${VERSION#v}"

GPG_PRIVATE_KEY_B64="${GPG_PRIVATE_KEY_B64:?GPG_PRIVATE_KEY_B64 must be set}"
APK_PRIVATE_KEY_B64="${APK_PRIVATE_KEY_B64:?APK_PRIVATE_KEY_B64 must be set}"
APK_PUBLIC_KEY_B64="${APK_PUBLIC_KEY_B64:?APK_PUBLIC_KEY_B64 must be set}"
GPG_PASSPHRASE="${GPG_PASSPHRASE:-}"
NFPM_APK_PASSPHRASE="${NFPM_APK_PASSPHRASE:-}"

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "${WORK_DIR}"' EXIT

GNUPGHOME="${WORK_DIR}/gnupg"
export GNUPGHOME
mkdir -p "${GNUPGHOME}"
chmod 700 "${GNUPGHOME}"

KEYS_DIR="${OUTPUT_DIR}/keys"
APT_ROOT="${OUTPUT_DIR}/apt"
RPM_ROOT="${OUTPUT_DIR}/rpm"
APK_ROOT="${OUTPUT_DIR}/apk"
PACKAGE_ROOT="${OUTPUT_DIR}/packages"
mkdir -p "${KEYS_DIR}" "${APT_ROOT}" "${RPM_ROOT}" "${APK_ROOT}" "${PACKAGE_ROOT}"

GPG_KEY_FILE="${WORK_DIR}/signing.asc"
APK_KEY_FILE="${WORK_DIR}/lakesql-packages.rsa"
APK_PUBLIC_KEY_FILE="${KEYS_DIR}/lakesql-packages.rsa.pub"

printf '%s' "${GPG_PRIVATE_KEY_B64}" | base64 --decode > "${GPG_KEY_FILE}"
printf '%s' "${APK_PRIVATE_KEY_B64}" | base64 --decode > "${APK_KEY_FILE}"
printf '%s' "${APK_PUBLIC_KEY_B64}" | base64 --decode > "${APK_PUBLIC_KEY_FILE}"
chmod 600 "${GPG_KEY_FILE}" "${APK_KEY_FILE}"

gpg --batch --import "${GPG_KEY_FILE}" >/dev/null 2>&1
GPG_KEY_ID="$(gpg --batch --list-secret-keys --with-colons | awk -F: '$1 == "sec" { print $5; exit }')"
if [[ -z "${GPG_KEY_ID}" ]]; then
  echo "Failed to resolve GPG key id" >&2
  exit 1
fi

gpg --batch --armor --export "${GPG_KEY_ID}" > "${KEYS_DIR}/lakesql-archive-keyring.asc"
gpg --batch --export "${GPG_KEY_ID}" > "${KEYS_DIR}/lakesql-archive-keyring.gpg"
cp "${KEYS_DIR}/lakesql-archive-keyring.asc" "${KEYS_DIR}/RPM-GPG-KEY-lakesql"

build_target() {
  local rust_target="$1"
  local deb_arch="$2"
  local rpm_arch="$3"
  local apk_arch="$4"

  local archive="${DIST_DIR}/lakesql-${rust_target}.tar.gz"
  local unpack_dir="${WORK_DIR}/${rust_target}"
  local binary_path="${unpack_dir}/lakesql"

  mkdir -p "${unpack_dir}"
  tar -xzf "${archive}" -C "${unpack_dir}"

  export NFPM_VERSION="${SEMVER_VERSION}"
  export NFPM_SOURCE_BINARY="${binary_path}"
  export NFPM_GPG_KEY_FILE="${GPG_KEY_FILE}"
  export NFPM_APK_KEY_FILE="${APK_KEY_FILE}"
  export NFPM_APK_KEY_NAME="$(basename "${APK_PUBLIC_KEY_FILE}")"
  export NFPM_PASSPHRASE="${GPG_PASSPHRASE}"
  export NFPM_RPM_PASSPHRASE="${GPG_PASSPHRASE}"
  export NFPM_DEB_PASSPHRASE="${GPG_PASSPHRASE}"
  export NFPM_APK_PASSPHRASE="${NFPM_APK_PASSPHRASE}"

  local deb_pkg="${PACKAGE_ROOT}/lakesql_${SEMVER_VERSION}_${deb_arch}.deb"
  local rpm_pkg="${PACKAGE_ROOT}/lakesql-${SEMVER_VERSION}-1.${rpm_arch}.rpm"
  local apk_pkg="${PACKAGE_ROOT}/lakesql-${SEMVER_VERSION}-r0.${apk_arch}.apk"

  NFPM_ARCH="${deb_arch}" nfpm package --config "${ROOT_DIR}/nfpm.yaml" --packager deb --target "${deb_pkg}"
  NFPM_ARCH="${rpm_arch}" nfpm package --config "${ROOT_DIR}/nfpm.yaml" --packager rpm --target "${rpm_pkg}"
  NFPM_ARCH="${apk_arch}" nfpm package --config "${ROOT_DIR}/nfpm.yaml" --packager apk --target "${apk_pkg}"
}

build_target x86_64-unknown-linux-musl amd64 x86_64 x86_64
build_target aarch64-unknown-linux-musl arm64 aarch64 aarch64

mkdir -p \
  "${APT_ROOT}/pool/main/l/lakesql" \
  "${APT_ROOT}/dists/stable/main/binary-amd64" \
  "${APT_ROOT}/dists/stable/main/binary-arm64"
cp "${PACKAGE_ROOT}"/*.deb "${APT_ROOT}/pool/main/l/lakesql/"

(
  cd "${APT_ROOT}"
  dpkg-scanpackages -a amd64 pool/main/l/lakesql > dists/stable/main/binary-amd64/Packages
  gzip -9fk dists/stable/main/binary-amd64/Packages
  dpkg-scanpackages -a arm64 pool/main/l/lakesql > dists/stable/main/binary-arm64/Packages
  gzip -9fk dists/stable/main/binary-arm64/Packages
  cat > apt-release.conf <<'EOF'
APT::FTPArchive::Release {
  Origin "TiDB Cloud";
  Label "LakeSQL";
  Suite "stable";
  Codename "stable";
  Architectures "amd64 arm64";
  Components "main";
  Description "LakeSQL stable packages";
};
EOF
  apt-ftparchive -c apt-release.conf release dists/stable > dists/stable/Release
  gpg --batch --yes --pinentry-mode loopback --passphrase "${GPG_PASSPHRASE}" \
    --default-key "${GPG_KEY_ID}" --clearsign \
    --output dists/stable/InRelease dists/stable/Release
  gpg --batch --yes --pinentry-mode loopback --passphrase "${GPG_PASSPHRASE}" \
    --default-key "${GPG_KEY_ID}" --armor --detach-sign \
    --output dists/stable/Release.gpg dists/stable/Release
)

for rpm_arch in x86_64 aarch64; do
  mkdir -p "${RPM_ROOT}/stable/${rpm_arch}"
  cp "${PACKAGE_ROOT}"/*.${rpm_arch}.rpm "${RPM_ROOT}/stable/${rpm_arch}/"
  createrepo_c "${RPM_ROOT}/stable/${rpm_arch}"
  gpg --batch --yes --pinentry-mode loopback --passphrase "${GPG_PASSPHRASE}" \
    --default-key "${GPG_KEY_ID}" --armor --detach-sign \
    --output "${RPM_ROOT}/stable/${rpm_arch}/repodata/repomd.xml.asc" \
    "${RPM_ROOT}/stable/${rpm_arch}/repodata/repomd.xml"
done

for apk_arch in x86_64 aarch64; do
  mkdir -p "${APK_ROOT}/stable/${apk_arch}"
  cp "${PACKAGE_ROOT}"/*.${apk_arch}.apk "${APK_ROOT}/stable/${apk_arch}/"
  docker run --rm \
    -v "${APK_ROOT}/stable/${apk_arch}:/repo" \
    -v "${APK_KEY_FILE}:/keys/lakesql-packages.rsa:ro" \
    -v "${APK_PUBLIC_KEY_FILE}:/keys/lakesql-packages.rsa.pub:ro" \
    alpine:3.22 sh -euxc "
      apk add --no-cache abuild
      apk index --allow-untrusted --output /repo/APKINDEX.tar.gz /repo/*.apk
      abuild-sign -k /keys/lakesql-packages.rsa -p lakesql-packages.rsa.pub /repo/APKINDEX.tar.gz
    "
done
