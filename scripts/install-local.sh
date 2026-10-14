#!/usr/bin/env bash
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
level="x86-64-v3"
target_dir="$root/target/$level"
dest="${FERRITE_BIN_DIR:-$HOME/.local/bin}/ferrite"

if ! /lib/ld-linux-x86-64.so.2 --help 2>/dev/null | grep -q -- "$level (supported"; then
    echo "This CPU does not support $level; use: cargo build --release -p rusty-app" >&2
    exit 1
fi

echo "Building ferrite for $level into $target_dir"
RUSTFLAGS="-C target-cpu=$level" cargo build --release -p rusty-app --manifest-path "$root/Cargo.toml" --target-dir "$target_dir"

echo "Installing $dest"
install -Dm755 -- "$target_dir/release/ferrite" "$dest"
"$dest"
