#!/usr/bin/env bash
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
level="x86-64-v3"
target_dir="$root/target/$level"
dest="${FERRITE_BIN_DIR:-$HOME/.local/bin}/ferrite"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
brand="$root/assets/brand"

if ! /lib/ld-linux-x86-64.so.2 --help 2>/dev/null | grep -q -- "$level (supported"; then
    echo "This CPU does not support $level; use: cargo build --release -p rusty-app" >&2
    exit 1
fi

echo "Building ferrite for $level into $target_dir"
RUSTFLAGS="-C target-cpu=$level" cargo build --release -p rusty-app --manifest-path "$root/Cargo.toml" --target-dir "$target_dir"

echo "Installing $dest"
install -Dm755 -- "$target_dir/release/ferrite" "$dest"

echo "Installing icons and the desktop entry into $data_dir"
icons="$data_dir/icons/hicolor"
for n in 16 24 32 48 64 128 256; do
    install -Dm644 -- "$brand/png/ferrite-$n.png" "$icons/${n}x${n}/apps/ferrite.png"
done
install -Dm644 -- "$brand/ferrite.svg" "$icons/scalable/apps/ferrite.svg"
mkdir -p "$data_dir/applications"
sed "s|^Exec=ferrite\$|Exec=$dest|" "$brand/ferrite.desktop" > "$data_dir/applications/ferrite.desktop"
gtk-update-icon-cache -q -t "$icons" 2>/dev/null || true
update-desktop-database -q "$data_dir/applications" 2>/dev/null || true

"$dest"
