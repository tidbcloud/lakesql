#!/bin/bash

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

# install/lakesql.sh
#
# Install LakeSQL from the S3-backed release bucket.
# Based on the Databend bendsql installer, but updated to use the
# LakeSQL release layout and naming.

set -u

PACKAGE_ROOT="${PACKAGE_ROOT:-"https://lakesql-bin.tidbcloud.com/lakesql"}"
LAKESQL_VERSION="${LAKESQL_VERSION:-""}"
_divider="--------------------------------------------------------------------------------"
_prompt=">>>"
_indent="   "

header() {
  cat 1>&2 <<EOF

LakeSQL Installer
$_divider
Repository: https://github.com/tidbcloud/lakesql
Downloads: ${PACKAGE_ROOT}
$_divider

EOF
}

usage() {
  cat 1>&2 <<EOF
lakesql-install
The installer for LakeSQL (https://github.com/tidbcloud/lakesql)

USAGE:
    lakesql-install [FLAGS] [OPTIONS]

FLAGS:
    -y                      Disable confirmation prompt.
        --prefix <DIR>      The directory where the files should be placed, default: "$HOME/.lakesql"
                            Note: This option automatically assumes the \`--no-modify-path\` flag
        --no-modify-path    Don't configure the PATH environment variable
    -h, --help              Prints help information
EOF
}

main() {
  downloader --check
  header

  local prompt=yes
  local modify_path=yes
  local prefix="$HOME/.lakesql"

  while [ $# -gt 0 ]; do
    case "$1" in
      -h|--help)
        usage
        exit 0
        ;;
      --prefix)
        shift
        [ $# -gt 0 ] || err "missing value for --prefix"
        prefix="$1"
        modify_path=no
        ;;
      --no-modify-path)
        modify_path=no
        ;;
      -y)
        prompt=no
        ;;
      *)
        err "unknown option: $1"
        ;;
    esac
    shift
  done

  resolve_version
  local version="$RETVAL"

  if [ "$prompt" = "yes" ]; then
    echo "$_prompt We'll be installing LakeSQL via a pre-built archive at ${PACKAGE_ROOT}/${version}/"
    echo "$_prompt Ready to proceed? (y/n)"
    echo ""

    while true; do
      read -rp "$_prompt " _choice </dev/tty
      case "$_choice" in
        n)
          err "exiting"
          ;;
        y)
          break
          ;;
        *)
          echo "Please enter y or n."
          ;;
      esac
    done

    echo ""
    echo "$_divider"
    echo ""
  fi

  install_from_archive "$modify_path" "$prefix" "$version"
}

resolve_version() {
  if [ -n "${LAKESQL_VERSION}" ]; then
    RETVAL="${LAKESQL_VERSION}"
    return 0
  fi

  need_cmd mktemp
  need_cmd rm
  need_cmd rmdir
  need_cmd sed
  need_cmd head

  local dir
  dir="$(mktemp -d 2>/dev/null || ensure mktemp -d -t lakesql-install)"
  local latest_file="${dir}/latest.json"
  local latest_url="${PACKAGE_ROOT}/latest.json"

  printf "%s Resolving latest LakeSQL version from %s" "$_prompt" "$latest_url"
  ensure downloader "$latest_url" "$latest_file" "latest metadata"
  printf " done\n"

  local version
  version="$(sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$latest_file" | head -n 1)"
  [ -n "$version" ] || err "failed to parse version from ${latest_url}"

  ignore rm "$latest_file"
  ignore rmdir "$dir"

  RETVAL="$version"
}

install_from_archive() {
  need_cmd grep
  need_cmd mkdir
  need_cmd mktemp
  need_cmd rm
  need_cmd rmdir
  need_cmd sed
  need_cmd tar
  need_cmd awk

  get_architecture || return 1
  local modify_path="$1"
  local prefix="$2"
  local version="$3"
  local arch="$RETVAL"
  local archive_arch=""

  case "$arch" in
    x86_64-apple-darwin)
      archive_arch="$arch"
      ;;
    x86_64-*linux*-gnu)
      archive_arch="x86_64-unknown-linux-gnu"
      ;;
    x86_64-*linux*-musl)
      archive_arch="x86_64-unknown-linux-musl"
      ;;
    aarch64-apple-darwin)
      archive_arch="$arch"
      ;;
    aarch64-*linux*)
      archive_arch="aarch64-unknown-linux-musl"
      ;;
    *)
      err "unsupported arch: $arch"
      ;;
  esac

  local url="${PACKAGE_ROOT}/${version}/lakesql-${archive_arch}.tar.gz"
  local checksums_url="${PACKAGE_ROOT}/${version}/checksums.txt"
  local dir
  dir="$(mktemp -d 2>/dev/null || ensure mktemp -d -t lakesql-install)"

  local archive_file="${dir}/lakesql-${archive_arch}.tar.gz"
  local checksums_file="${dir}/checksums.txt"

  ensure mkdir -p "$dir"

  printf "%s Downloading LakeSQL archive via %s" "$_prompt" "$url"
  ensure downloader "$url" "$archive_file" "$arch"
  printf " done\n"

  printf "%s Downloading checksums via %s" "$_prompt" "$checksums_url"
  ensure downloader "$checksums_url" "$checksums_file" "$arch"
  printf " done\n"

  verify_checksum "$checksums_file" "$archive_file" "lakesql-${archive_arch}.tar.gz"

  ensure mkdir -p "${prefix}/bin"

  printf "%s Unpacking archive to %s ..." "$_prompt" "$prefix"
  ensure tar -xzf "$archive_file" --directory="${prefix}/bin"
  printf " done\n"

  if [ "$modify_path" = "yes" ]; then
    local path_export="export PATH=\"\$PATH:$prefix/bin\""
    add_to_path "${HOME}/.zprofile" "${path_export}"
    add_to_path "${HOME}/.profile" "${path_export}"
  fi

  printf "%s Install succeeded!\n" "$_prompt"
  printf "%s To start LakeSQL:\n" "$_prompt"
  printf "\n"
  printf "%s lakesql --help\n" "$_indent"
  printf "\n"
  printf "%s More information at https://github.com/tidbcloud/lakesql\n" "$_prompt"

  local retval=$?

  ignore rm "$archive_file"
  ignore rm "$checksums_file"
  ignore rmdir "$dir"

  return "$retval"
}

verify_checksum() {
  need_cmd grep
  need_cmd head
  need_cmd awk

  local checksums_file="$1"
  local archive_file="$2"
  local archive_name="$3"
  local expected
  expected="$(grep " ${archive_name}\$" "$checksums_file" | head -n 1 | awk '{print $1}')"
  [ -n "$expected" ] || err "missing checksum entry for ${archive_name}"

  local actual
  actual="$(compute_sha256 "$archive_file")"
  [ "$actual" = "$expected" ] || err "checksum mismatch for ${archive_name}"

  printf "%s Verified checksum for %s\n" "$_prompt" "$archive_name"
}

compute_sha256() {
  need_cmd awk

  if check_cmd sha256sum; then
    sha256sum "$1" | awk '{print $1}'
  elif check_cmd shasum; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    err "need 'sha256sum' or 'shasum' (command not found)"
  fi
}

add_to_path() {
  local file="$1"
  local new_path="$2"

  printf "%s Adding LakeSQL path to %s" "$_prompt" "$file"

  if [ ! -f "$file" ]; then
    echo "${new_path}" >> "${file}"
  else
    grep -qxF "${new_path}" "${file}" || echo "${new_path}" >> "${file}"
  fi

  printf " done\n"
}

# ------------------------------------------------------------------------------
# The platform detection helpers below are adapted from the Databend installer
# and from rustup's shell installer.
# ------------------------------------------------------------------------------

check_proc() {
  if ! test -L /proc/self/exe; then
    err "fatal: Unable to find /proc/self/exe. Is /proc mounted? Installation cannot proceed without /proc."
  fi
}

get_bitness() {
  need_cmd head
  local current_exe_head
  current_exe_head="$(head -c 5 /proc/self/exe)"
  if [ "$current_exe_head" = "$(printf '\177ELF\001')" ]; then
    echo 32
  elif [ "$current_exe_head" = "$(printf '\177ELF\002')" ]; then
    echo 64
  else
    err "unknown platform bitness"
  fi
}

get_endianness() {
  local cputype="$1"
  local suffix_eb="$2"
  local suffix_el="$3"

  need_cmd head
  need_cmd tail

  local current_exe_endianness
  current_exe_endianness="$(head -c 6 /proc/self/exe | tail -c 1)"
  if [ "$current_exe_endianness" = "$(printf '\001')" ]; then
    echo "${cputype}${suffix_el}"
  elif [ "$current_exe_endianness" = "$(printf '\002')" ]; then
    echo "${cputype}${suffix_eb}"
  else
    err "unknown platform endianness"
  fi
}

get_architecture() {
  local ostype cputype bitness arch clibtype
  ostype="$(uname -s)"
  cputype="$(uname -m)"
  clibtype="gnu"

  if [ "$ostype" = Linux ]; then
    if [ "$(uname -o)" = Android ]; then
      ostype=Android
    fi
    if ldd --version 2>&1 | grep -q "musl"; then
      clibtype="musl"
    fi
  fi

  if [ "$ostype" = Darwin ] && [ "$cputype" = i386 ]; then
    if sysctl hw.optional.x86_64 | grep -q ": 1"; then
      cputype=x86_64
    fi
  fi

  if [ "$ostype" = SunOS ]; then
    if [ "$(/usr/bin/uname -o)" = illumos ]; then
      ostype=illumos
    fi

    if [ "$cputype" = i86pc ]; then
      cputype="$(isainfo -n)"
    fi
  fi

  case "$ostype" in
    Android)
      ostype=linux-android
      ;;
    Linux)
      check_proc
      ostype=unknown-linux-${clibtype}
      bitness="$(get_bitness)"
      ;;
    FreeBSD)
      ostype=unknown-freebsd
      ;;
    NetBSD)
      ostype=unknown-netbsd
      ;;
    DragonFly)
      ostype=unknown-dragonfly
      ;;
    Darwin)
      ostype=apple-darwin
      ;;
    illumos)
      ostype=unknown-illumos
      ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT)
      ostype=pc-windows-gnu
      ;;
    *)
      err "unrecognized OS type: $ostype"
      ;;
  esac

  case "$cputype" in
    i386|i486|i686|i786|x86)
      cputype=i686
      ;;
    xscale|arm)
      cputype=arm
      if [ "$ostype" = "linux-android" ]; then
        ostype=linux-androideabi
      fi
      ;;
    armv6l)
      cputype=arm
      if [ "$ostype" = "linux-android" ]; then
        ostype=linux-androideabi
      else
        ostype="${ostype}eabihf"
      fi
      ;;
    armv7l|armv8l)
      cputype=armv7
      if [ "$ostype" = "linux-android" ]; then
        ostype=linux-androideabi
      else
        ostype="${ostype}eabihf"
      fi
      ;;
    aarch64|arm64)
      cputype=aarch64
      ;;
    x86_64|x86-64|x64|amd64)
      cputype=x86_64
      ;;
    mips)
      cputype="$(get_endianness mips '' el)"
      ;;
    mips64)
      if [ "$bitness" -eq 64 ]; then
        ostype="${ostype}abi64"
        cputype="$(get_endianness mips64 '' el)"
      fi
      ;;
    ppc)
      cputype=powerpc
      ;;
    ppc64)
      cputype=powerpc64
      ;;
    ppc64le)
      cputype=powerpc64le
      ;;
    s390x)
      cputype=s390x
      ;;
    riscv64)
      cputype=riscv64gc
      ;;
    *)
      err "unknown CPU type: $cputype"
      ;;
  esac

  if [ "${ostype}" = unknown-linux-gnu ] && [ "${bitness}" -eq 32 ]; then
    case "$cputype" in
      x86_64)
        if [ -n "${RUSTUP_CPUTYPE:-}" ]; then
          cputype="$RUSTUP_CPUTYPE"
        else
          if is_host_amd64_elf; then
            echo "This host is running an x32 userland; as it stands, x32 support is poor," 1>&2
            echo "and there isn't a native toolchain -- you will have to install" 1>&2
            echo "multiarch compatibility with i686 and/or amd64, then select one" 1>&2
            echo "by re-running this script with the RUSTUP_CPUTYPE environment variable" 1>&2
            echo "set to i686 or x86_64, respectively." 1>&2
            echo 1>&2
            echo "You will be able to add an x32 target after installation by running" 1>&2
            echo "  rustup target add x86_64-unknown-linux-gnux32" 1>&2
            exit 1
          else
            cputype=i686
          fi
        fi
        ;;
      mips64)
        cputype="$(get_endianness mips '' el)"
        ;;
      powerpc64)
        cputype=powerpc
        ;;
      aarch64)
        cputype=armv7
        if [ "$ostype" = "linux-android" ]; then
          ostype=linux-androideabi
        else
          ostype="${ostype}eabihf"
        fi
        ;;
      riscv64gc)
        err "riscv64 with 32-bit userland unsupported"
        ;;
    esac
  fi

  if [ "$ostype" = "unknown-linux-gnueabihf" ] && [ "$cputype" = armv7 ]; then
    if ensure grep "^Features" /proc/cpuinfo | grep -q -v neon; then
      cputype=arm
    fi
  fi

  arch="${cputype}-${ostype}"
  RETVAL="$arch"
}

say() {
  printf 'lakesql: %s\n' "$1"
}

err() {
  say "$1" >&2
  exit 1
}

need_cmd() {
  if ! check_cmd "$1"; then
    err "need '$1' (command not found)"
  fi
}

check_cmd() {
  command -v "$1" >/dev/null 2>&1
}

ensure() {
  if ! "$@"; then
    err "command failed: $*"
  fi
}

ignore() {
  "$@"
}

downloader() {
  local dld
  local err_output
  local status
  local retry

  if check_cmd curl; then
    dld=curl
  elif check_cmd wget; then
    dld=wget
  else
    dld="curl or wget"
  fi

  if [ "$1" = --check ]; then
    need_cmd "$dld"
  elif [ "$dld" = curl ]; then
    check_curl_for_retry_support
    retry="$RETVAL"
    err_output="$(curl $retry --silent --show-error --fail --location "$1" --output "$2" 2>&1)"
    status=$?
    if [ -n "$err_output" ]; then
      echo "$err_output" >&2
      if echo "$err_output" | grep -q "404$"; then
        err "installer for platform '$3' not found, this may be unsupported"
      fi
    fi
    return "$status"
  elif [ "$dld" = wget ]; then
    err_output="$(wget "$1" -O "$2" 2>&1)"
    status=$?
    if [ -n "$err_output" ]; then
      echo "$err_output" >&2
      if echo "$err_output" | grep -q " 404 Not Found$"; then
        err "installer for platform '$3' not found, this may be unsupported"
      fi
    fi
    return "$status"
  else
    err "Unknown downloader"
  fi
}

check_help_for() {
  local arch
  local cmd
  local arg

  arch="$1"
  shift
  cmd="$1"
  shift

  local category
  if "$cmd" --help | grep -q 'For all options use the manual or "--help all".'; then
    category="all"
  else
    category=""
  fi

  case "$arch" in
    *darwin*)
      if check_cmd sw_vers; then
        case "$(sw_vers -productVersion)" in
          10.*)
            if [ "$(sw_vers -productVersion | cut -d. -f2)" -lt 13 ]; then
              echo "Warning: Detected macOS platform older than 10.13"
              return 1
            fi
            ;;
          11.*)
            ;;
          *)
            echo "Warning: Detected unknown macOS major version: $(sw_vers -productVersion)"
            echo "Warning TLS capabilities detection may fail"
            ;;
        esac
      fi
      ;;
  esac

  for arg in "$@"; do
    if ! "$cmd" --help "$category" | grep -q -- "$arg"; then
      return 1
    fi
  done
}

check_curl_for_retry_support() {
  local retry_supported=""
  if check_help_for "notspecified" "curl" "--retry"; then
    retry_supported="--retry 3"
  fi

  RETVAL="$retry_supported"
}

is_host_amd64_elf() {
  local machine
  machine="$(uname -m)"
  [ "${machine}" = "x86_64" ] || return 1

  if [ ! -L /proc/self/exe ]; then
    return 1
  fi

  local current_exe_head
  current_exe_head="$(head -c 5 /proc/self/exe 2>/dev/null || true)"
  [ "${current_exe_head}" = "$(printf '\177ELF\002')" ]
}

main "$@" || exit 1
