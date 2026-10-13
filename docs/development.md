# Development and packaging

Requirements: a Mac, Xcode/Command Line Tools, and Rust with Cargo in PATH. The development environment uses Rust 1.99, Apple Silicon, and the versions in Cargo.lock. There are no dependencies on personal paths in another checkout. `READY_AIR_RUST_TOOLCHAIN` can point to a directory containing `bin/cargo`; a local selection can also be saved in `.tools/rust-toolchain-path`, which Git ignores.

```sh
git clone https://github.com/facundoPri/air-controller-adapter.git
cd air-controller-adapter
./script/cargo.sh test --locked --all-targets
./script/build_and_run.sh --build-only
```

The packaging script creates `dist/ACA Studio.app` and `dist/ACA Capture.app`. Without `--build-only`, it opens Studio. `READY_AIR_PROFILE=release` enables an optimized build, and `READY_AIR_DIST` changes the output directory. Internal crate and executable names preserve the previous identity for continuity.

```sh
READY_AIR_PROFILE=release READY_AIR_DIST="$PWD/dist/release" ./script/build_and_run.sh --build-only
"dist/release/ACA Studio.app/Contents/MacOS/ReadyForAirStudio" --verify-capture
```

Order matters: Capture is built and signed first, and its hash is embedded when compiling Studio. Studio is then signed. Building Studio directly with Cargo works for development but does not create a matched pair authorized to start the reader. `--verify-capture` checks the pair without opening the UI, starting capture, or requesting privileges.

Signing is ad hoc by default. `READY_AIR_SIGN_IDENTITY` selects an installed identity; this alone does not add notarization or certify the distribution workflow.

Tests check HID reports, queues, cancellation, persistence, drafts, observed permissions, and reader startup in isolation. They do not grant permissions, test actual desktop effects, or replace installation testing on another Mac. Optional renderer and Zoom preference tests are skipped by default.

The preview download contains two app bundles, README, and dependency notices. It excludes local data, hardware logs, coordination notes, and third-party reference material.

To create a ZIP from an already-built optimized pair, with the source commit recorded, signatures verified before and after extraction, and a SHA-256 checksum:

```sh
./script/package_release.sh v0.1.0-preview.2
```

Output goes to `dist/releases/v0.1.0-preview.2/`. `THIRD_PARTY_NOTICES.md` corresponds to Cargo.lock and the macOS arm64 dependency tree, including build tools. Update its inventory and original license texts when changing dependencies. Preserve upstream license wording and author names.
