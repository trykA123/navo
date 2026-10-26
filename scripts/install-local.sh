#!/usr/bin/env bash
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
level="x86-64-v3"
target_dir="$root/target/$level"
dest="${NAVO_BIN_DIR:-$HOME/.local/bin}/navo"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
brand="$root/assets/brand"

if ! /lib/ld-linux-x86-64.so.2 --help 2>/dev/null | grep -q -- "$level (supported"; then
    echo "This CPU does not support $level; use: cargo build --release -p rusty-app" >&2
    exit 1
fi

echo "Building navo for $level into $target_dir"
RUSTFLAGS="-C target-cpu=$level" cargo build --release -p rusty-app --manifest-path "$root/Cargo.toml" --target-dir "$target_dir"

echo "Installing $dest"
install -Dm755 -- "$target_dir/release/navo" "$dest"

echo "Installing icons and the desktop entry into $data_dir"
icons="$data_dir/icons/hicolor"
for n in 16 24 32 48 64 128 256; do
    install -Dm644 -- "$brand/png/navo-$n.png" "$icons/${n}x${n}/apps/navo.png"
done
install -Dm644 -- "$brand/navo.svg" "$icons/scalable/apps/navo.svg"
mkdir -p "$data_dir/applications"
sed "s|^Exec=navo\$|Exec=$dest|" "$brand/navo.desktop" > "$data_dir/applications/navo.desktop"
gtk-update-icon-cache -q -t "$icons" 2>/dev/null || true
update-desktop-database -q "$data_dir/applications" 2>/dev/null || true

"$dest"
