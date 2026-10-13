#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
export CARGO_HOME="$ROOT/.tools/cargo"
# A local, ignored path may preserve a developer's toolchain without baking it
# into the repository. Fresh checkouts use Cargo in PATH or the explicit override.
TOOLCHAIN="${READY_AIR_RUST_TOOLCHAIN:-}"
if [ -z "$TOOLCHAIN" ] && [ -f "$ROOT/.tools/rust-toolchain-path" ]; then
  IFS= read -r TOOLCHAIN < "$ROOT/.tools/rust-toolchain-path"
fi
if [ -n "$TOOLCHAIN" ]; then
  if [ ! -x "$TOOLCHAIN/bin/cargo" ]; then
    echo "READY_AIR_RUST_TOOLCHAIN does not contain an executable bin/cargo" >&2
    exit 1
  fi
  export PATH="$TOOLCHAIN/bin:$PATH"
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "Install Rust and add cargo to PATH, or set READY_AIR_RUST_TOOLCHAIN. macOS also requires Xcode/Command Line Tools." >&2
  exit 1
fi
cd "$ROOT"
exec cargo "$@"
