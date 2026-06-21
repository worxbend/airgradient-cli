#!/usr/bin/env bash

set -euo pipefail

DEFAULT_TARGET="x86_64-unknown-linux-gnu"
TARGET="$DEFAULT_TARGET"
OUTPUT_DIR="dist"
BIN_NAME="airgradient-cli"
SKIP_BUILD=false
BINARY_OVERRIDE=""

usage() {
  cat <<'USAGE'
Usage: scripts/release-dry-run.sh [--target <triple>] [--output-dir <dir>] [--skip-build --binary <path>]

Builds and stages the Linux release artifact locally.
This is a dry run only: it does not tag, publish, upload, sign, or generate shell completions.
USAGE
}

fail() {
  printf 'release dry run failed: %s\n' "$1" >&2
  exit 1
}

while (($# > 0)); do
  case "$1" in
    --target)
      (($# >= 2)) || fail "unsupported invocation: --target requires a value"
      TARGET="$2"
      shift 2
      ;;
    --output-dir)
      (($# >= 2)) || fail "unsupported invocation: --output-dir requires a value"
      OUTPUT_DIR="$2"
      shift 2
      ;;
    --skip-build)
      SKIP_BUILD=true
      shift
      ;;
    --binary)
      (($# >= 2)) || fail "unsupported invocation: --binary requires a value"
      BINARY_OVERRIDE="$2"
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      fail "unsupported invocation: unexpected argument '$1'"
      ;;
  esac
done

[[ -n "$TARGET" ]] || fail "unsupported invocation: --target must not be empty"
[[ -n "$OUTPUT_DIR" ]] || fail "unsupported invocation: --output-dir must not be empty"
if [[ "$TARGET" != "$DEFAULT_TARGET" ]]; then
  fail "unsupported target '$TARGET': first-release dry run supports only $DEFAULT_TARGET"
fi
if [[ "$SKIP_BUILD" == false && -n "$BINARY_OVERRIDE" ]]; then
  fail "unsupported invocation: --binary requires --skip-build"
fi
if [[ "$SKIP_BUILD" == true && -z "$BINARY_OVERRIDE" ]]; then
  fail "unsupported invocation: --skip-build requires --binary <path>"
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

[[ -f "LICENSE" ]] || fail "missing checked-in LICENSE at $REPO_ROOT/LICENSE"
[[ -f "Cargo.toml" ]] || fail "missing Cargo.toml at $REPO_ROOT/Cargo.toml"

if [[ -e "$OUTPUT_DIR" && ! -d "$OUTPUT_DIR" ]]; then
  fail "output path exists but is not a directory: $OUTPUT_DIR"
fi
if [[ -d "$OUTPUT_DIR" ]] && find "$OUTPUT_DIR" -mindepth 1 -print -quit | grep -q .; then
  fail "output directory must be absent or empty before release dry run: $OUTPUT_DIR"
fi

VERSION="$(
  awk -F '=' '
    /^\[package\][[:space:]]*$/ { in_package = 1; next }
    /^\[/ { if (in_package) exit }
    in_package && $1 ~ /^[[:space:]]*version[[:space:]]*$/ {
      value = $2
      sub(/#.*/, "", value)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", value)
      gsub(/^"|"$/, "", value)
      print value
      exit
    }
  ' Cargo.toml
)"
[[ -n "$VERSION" ]] || fail "missing package version in Cargo.toml"

if command -v sha256sum >/dev/null 2>&1; then
  CHECKSUM_CMD=(sha256sum)
elif command -v shasum >/dev/null 2>&1; then
  CHECKSUM_CMD=(shasum -a 256)
else
  fail "missing checksum tool: install sha256sum or shasum"
fi

if [[ "$SKIP_BUILD" == true ]]; then
  BINARY_PATH="$BINARY_OVERRIDE"
  [[ -f "$BINARY_PATH" ]] || fail "missing binary for --skip-build: $BINARY_PATH"
  [[ -x "$BINARY_PATH" ]] || fail "binary for --skip-build is not executable: $BINARY_PATH"
else
  cargo build --release --target "$TARGET"
  BINARY_PATH="target/$TARGET/release/$BIN_NAME"
  [[ -f "$BINARY_PATH" ]] || fail "missing built binary after build: $BINARY_PATH"
fi

ARTIFACT_NAME="$BIN_NAME-v$VERSION-$TARGET.tar.gz"
OUTPUT_DIR_ABS="$(mkdir -p -- "$OUTPUT_DIR" && cd -- "$OUTPUT_DIR" && pwd)"
ARTIFACT_PATH="$OUTPUT_DIR_ABS/$ARTIFACT_NAME"
CHECKSUMS_PATH="$OUTPUT_DIR_ABS/SHA256SUMS"
STAGING_DIR="$(mktemp -d)"

cleanup() {
  rm -rf -- "$STAGING_DIR"
}
trap cleanup EXIT

cp -- "$BINARY_PATH" "$STAGING_DIR/$BIN_NAME"
cp -- "LICENSE" "$STAGING_DIR/LICENSE"

tar -C "$STAGING_DIR" -czf "$ARTIFACT_PATH" "$BIN_NAME" "LICENSE"

(
  cd "$OUTPUT_DIR_ABS"
  "${CHECKSUM_CMD[@]}" "$ARTIFACT_NAME" >"$CHECKSUMS_PATH"
)

printf 'Staged release dry run artifact: %s\n' "$ARTIFACT_PATH"
printf 'Staged release dry run checksums: %s\n' "$CHECKSUMS_PATH"
