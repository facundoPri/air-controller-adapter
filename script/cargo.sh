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
    echo "READY_AIR_RUST_TOOLCHAIN no contiene bin/cargo ejecutable" >&2
    exit 1
  fi
  export PATH="$TOOLCHAIN/bin:$PATH"
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "Instalá Rust y agregá cargo a PATH, o indicá READY_AIR_RUST_TOOLCHAIN. Para macOS también necesitás Xcode/Command Line Tools." >&2
  exit 1
fi
cd "$ROOT"
exec cargo "$@"
