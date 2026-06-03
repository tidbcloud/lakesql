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

usage() {
  cat <<'EOF'
Usage: render_homebrew_formula.sh --version <vX.Y.Z> --checksums-file <path> --output <path>

Render Formula/lakesql.rb from the release version and publish_s3 checksums.
EOF
}

err() {
  echo "$*" >&2
  exit 1
}

require_arg() {
  local name="$1"
  local value="$2"
  [[ -n "${value}" ]] || err "missing required argument: ${name}"
}

lookup_checksum() {
  local checksums_file="$1"
  local target="$2"
  local archive_name="lakesql-${target}.tar.gz"
  local checksum

  checksum="$(awk -v archive_name="${archive_name}" '$2 == archive_name { print $1; exit }' "${checksums_file}")"
  [[ -n "${checksum}" ]] || err "missing checksum entry for ${archive_name}"

  printf '%s' "${checksum}"
}

version=""
checksums_file=""
output=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      version="${2:-}"
      shift 2
      ;;
    --checksums-file)
      checksums_file="${2:-}"
      shift 2
      ;;
    --output)
      output="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      err "unknown argument: $1"
      ;;
  esac
done

require_arg "--version" "${version}"
require_arg "--checksums-file" "${checksums_file}"
require_arg "--output" "${output}"
[[ -f "${checksums_file}" ]] || err "checksums file not found: ${checksums_file}"

formula_version="${version#v}"
[[ "${formula_version}" != "${version}" ]] || err "version must start with 'v': ${version}"

arm_checksum="$(lookup_checksum "${checksums_file}" "aarch64-apple-darwin")"
intel_checksum="$(lookup_checksum "${checksums_file}" "x86_64-apple-darwin")"

mkdir -p "$(dirname "${output}")"

cat > "${output}" <<EOF
class Lakesql < Formula
  desc "TiDB Cloud Lake native command-line tool"
  homepage "https://github.com/tidbcloud/lakesql"
  version "${formula_version}"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://lakesql-bin.tidbcloud.com/lakesql/v#{version}/lakesql-aarch64-apple-darwin.tar.gz"
      sha256 "${arm_checksum}"
    end

    on_intel do
      url "https://lakesql-bin.tidbcloud.com/lakesql/v#{version}/lakesql-x86_64-apple-darwin.tar.gz"
      sha256 "${intel_checksum}"
    end
  end

  def install
    bin.install "lakesql"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/lakesql --version")
  end
end
EOF
