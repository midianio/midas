#!/usr/bin/env bash
# Idempotent Cursor Cloud Agent install for the midas CLI. Safe to re-run.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
export PATH="${HOME}/.cargo/bin:/usr/local/cargo/bin:${PATH}"

# rust-toolchain.toml pins a named channel, so rustup installs it on first cargo call. Never
# follow `stable` here: rustup renames snapshot-layer dirs on update and fails with EXDEV.
rustup show active-toolchain

# Warm the build cache so the first `cargo test` / `midas check` in a session is fast.
cargo fetch --locked
cargo build --workspace --locked

# Expose the in-tree binary as `midas` so the repo checks itself with the code under edit.
mkdir -p "${HOME}/.local/bin"
ln -sfn "${REPO_ROOT}/target/debug/midas" "${HOME}/.local/bin/midas"
if ! grep -q '# midas dev PATH' "${HOME}/.bashrc" 2>/dev/null; then
	printf '\n# midas dev PATH\nexport PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"\n' >>"${HOME}/.bashrc"
fi

echo "cloud install complete: rustc=$(rustc --version | awk '{print $2}') midas=$("${REPO_ROOT}/target/debug/midas" --version | awk '{print $2}')"
