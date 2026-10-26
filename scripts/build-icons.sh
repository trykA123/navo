#!/usr/bin/env bash
set -euo pipefail

brand="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../assets/brand" && pwd)"
mkdir -p "$brand/png"

cp -- "$brand/navo-16.png" "$brand/png/navo-16.png"
for n in 24 32 48; do
    rsvg-convert -w "$n" -h "$n" "$brand/navo-$n.svg" -o "$brand/png/navo-$n.png"
done
for n in 64 128 256; do
    rsvg-convert -w "$n" -h "$n" "$brand/navo.svg" -o "$brand/png/navo-$n.png"
done

python3 -I - "$brand/navo.ico" "$brand"/png/navo-{16,24,32,48,64,128,256}.png <<'PY'
import struct, sys
out, pngs = sys.argv[1], sys.argv[2:]
data = [open(p, "rb").read() for p in pngs]
entries, offset = b"", 6 + 16 * len(data)
for d in data:
    w, h = struct.unpack(">II", d[16:24])
    entries += struct.pack("<BBBBHHII", w % 256, h % 256, 0, 0, 1, 32, len(d), offset)
    offset += len(d)
with open(out, "wb") as f:
    f.write(struct.pack("<HHH", 0, 1, len(data)) + entries + b"".join(data))
PY

echo "Wrote $brand/png and $brand/navo.ico"
