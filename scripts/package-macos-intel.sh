#!/usr/bin/env bash
# One-click local packaging for the Intel (x86_64) macOS build of Pebrel.
#
# Builds the GPUI release binaries for x86_64 (natively on an Intel Mac, or
# cross-compiled on Apple Silicon) and packages them into a DMG through
# scripts/package-macos.sh. Without Developer ID credentials the DMG is
# ad-hoc signed and meant for local installation, not public distribution.
set -euo pipefail

export LC_ALL=en_US.UTF-8

usage() {
  cat <<'EOF'
Usage: scripts/package-macos-intel.sh [options]

Builds the Pebrel GPUI release binaries for Intel macOS (x86_64) and packages
them into a DMG. On an Intel Mac the binaries are built natively; on Apple
Silicon they are cross-compiled and packaged under Rosetta.

Options:
  --channel preview|stable   Package channel (default: preview)
  --preview-id ID            Preview identifier (default: local-<timestamp>)
  --build-number NUMBER      CFBundleVersion (default: commit count)
  --output-directory DIR     DMG output directory (default: dist)
  --force                    Replace an existing DMG with the same name
  --skip-build               Package the existing release binaries as-is
  --jobs N                   Cargo build jobs (default: cargo decides)
  --sign-identity ID         Developer ID Application identity
  --notary-profile PROFILE   notarytool keychain profile (needs --sign-identity)
  --signing-keychain PATH    Keychain holding the signing identity
  -h, --help                 Show this help

Examples:
  scripts/package-macos-intel.sh
  scripts/package-macos-intel.sh --channel stable --force
  scripts/package-macos-intel.sh --skip-build --output-directory tmp/dmg
EOF
}

channel="preview"
preview_id=""
build_number=""
output_directory="dist"
force=0
skip_build=0
jobs=""
sign_identity=""
notary_profile=""
signing_keychain=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --channel)
      channel="${2:-}"
      shift 2
      ;;
    --preview-id)
      preview_id="${2:-}"
      shift 2
      ;;
    --build-number)
      build_number="${2:-}"
      shift 2
      ;;
    --output-directory)
      output_directory="${2:-}"
      shift 2
      ;;
    --force)
      force=1
      shift
      ;;
    --skip-build)
      skip_build=1
      shift
      ;;
    --jobs)
      jobs="${2:-}"
      shift 2
      ;;
    --sign-identity)
      sign_identity="${2:-}"
      shift 2
      ;;
    --notary-profile)
      notary_profile="${2:-}"
      shift 2
      ;;
    --signing-keychain)
      signing_keychain="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "$channel" != "preview" && "$channel" != "stable" ]]; then
  echo "unsupported channel: $channel (expected preview or stable)" >&2
  exit 2
fi
if [[ -n "$jobs" && ! "$jobs" =~ ^[1-9][0-9]*$ ]]; then
  echo "--jobs must be a positive integer: $jobs" >&2
  exit 2
fi
if [[ -n "$build_number" && ! "$build_number" =~ ^[1-9][0-9]*$ ]]; then
  echo "--build-number must be a positive integer: $build_number" >&2
  exit 2
fi
if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "this script packages the macOS Intel build and must run on macOS" >&2
  exit 1
fi

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo"

# Prefer the rustup cargo: a Homebrew/MacPorts cargo can resolve to a
# different toolchain than rust-toolchain.toml pins.
if [[ -d "$HOME/.cargo/bin" ]]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required; install the pinned Rust toolchain first" >&2
  exit 1
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required by the packaging helpers" >&2
  exit 1
fi
pinned_toolchain="$(sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml)"
if [[ -n "$pinned_toolchain" ]]; then
  active_toolchain="$(rustc --version 2>/dev/null | awk '{print $2}')"
  if [[ -z "$active_toolchain" ]]; then
    echo "rustc is unavailable next to cargo" >&2
    exit 1
  fi
  if [[ "$active_toolchain" != "$pinned_toolchain" ]]; then
    echo "rustc $active_toolchain does not match rust-toolchain.toml ($pinned_toolchain)" >&2
    echo "put the rustup shim first on PATH (for example ~/.cargo/bin)" >&2
    exit 1
  fi
fi

# Intel host: build and package natively. Apple Silicon host: cross-compile and
# run the packager under Rosetta so its runner-architecture check passes.
host_arch="$(uname -m)"
rust_target=""
binary="target/release/pebrel"
runner=()
if [[ "$host_arch" == "arm64" ]]; then
  rust_target="x86_64-apple-darwin"
  binary="target/$rust_target/release/pebrel"
  runner=(arch -x86_64)
fi

version="$(python3 scripts/preview_release.py --print-package-version nebula_app/Cargo.toml)"

if [[ $skip_build -eq 0 ]]; then
  build_command=(cargo build --release --locked)
  if [[ -n "$rust_target" ]]; then
    if command -v rustup >/dev/null 2>&1; then
      rustup target add "$rust_target"
    fi
    build_command+=(--target "$rust_target")
  fi
  if [[ -n "$jobs" ]]; then
    build_command+=(--jobs "$jobs")
  fi
  build_command+=(-p nebula --bin pebrel -p nebula_hook --bin pebrel-hook
                  --features nebula/gpui-shell)
  echo "==> Building Pebrel $version for Intel macOS ($host_arch host)"
  "${build_command[@]}"
elif [[ ! -x "$binary" || ! -x "$(dirname "$binary")/pebrel-hook" ]]; then
  echo "release binaries are missing under $(dirname "$binary");" >&2
  echo "run without --skip-build to build them first" >&2
  exit 1
fi

if [[ -z "$build_number" ]]; then
  # Commit count is a deterministic, monotonic CFBundleVersion for local builds.
  if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    build_number="$(git rev-list --count HEAD)"
  fi
fi
if [[ ! "$build_number" =~ ^[1-9][0-9]*$ ]]; then
  build_number=1
fi

if [[ "$channel" == "preview" && -z "$preview_id" ]]; then
  preview_id="local-$(date +%Y%m%d-%H%M%S)"
fi

package_command=(${runner[@]+"${runner[@]}"} bash scripts/package-macos.sh
                 --channel "$channel"
                 --binary "$binary"
                 --version "$version"
                 --architecture x86_64
                 --build-number "$build_number"
                 --output-directory "$output_directory")
if [[ "$channel" == "preview" ]]; then
  package_command+=(--preview-id "$preview_id")
fi
if [[ $force -eq 1 ]]; then
  package_command+=(--force)
fi
if [[ -n "$sign_identity" ]]; then
  package_command+=(--sign-identity "$sign_identity")
fi
if [[ -n "$notary_profile" ]]; then
  package_command+=(--notary-profile "$notary_profile")
fi
if [[ -n "$signing_keychain" ]]; then
  package_command+=(--signing-keychain "$signing_keychain")
fi

echo "==> Packaging Intel DMG (channel=$channel, build=$build_number)"
"${package_command[@]}"

if [[ "$channel" == "stable" ]]; then
  asset_architecture="x64"
  release="$version"
else
  asset_architecture="x86_64"
  release="$version-preview.$preview_id"
fi
dmg="$output_directory/Pebrel-v$release-macos-$asset_architecture.dmg"
if [[ ! -s "$dmg" ]]; then
  echo "expected DMG is missing or empty: $dmg" >&2
  exit 1
fi

echo "==> Intel DMG ready"
echo "    path: $repo/$dmg"
echo "    size: $(stat -f%z "$dmg") bytes"
shasum -a 256 "$dmg"
open -R "$dmg" 2>/dev/null || true
