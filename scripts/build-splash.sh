#!/usr/bin/env bash
# Regenerate the splash masks: scripts/build-splash.sh (needs curl and python3 with Pillow).
# The committed masks were built with Pillow 13.0.0; other versions may rasterise differently.
set -euo pipefail

out="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../assets/brand" && pwd)/splash"
commit=03507d024a01282884232081fc6011c09ff4e849
base="https://raw.githubusercontent.com/floriankarsten/space-grotesk/$commit"
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT

curl -sSfL -o "$work/font.ttf" "$base/fonts/ttf/SpaceGrotesk%5Bwght%5D.ttf"
curl -sSfL -o "$work/OFL.txt" "$base/OFL.txt"
sha256sum -c - <<EOF
acad6de1fc93436f5c0f1f4137751ef04f1aea3063e7036535970ffcfbd79f72  $work/font.ttf
564ce565c371c5e5bbf286006565a7c9aa55a9f56e7ca58d56e05d649dd61a72  $work/OFL.txt
EOF

mkdir -p "$out"
cp "$work/OFL.txt" "$out/OFL.txt"

python3 -I - "$work/font.ttf" "$out" <<'PY'
import math, sys
import PIL
from PIL import Image, ImageDraw, ImageFont

font_path, out = sys.argv[1], sys.argv[2]
print(f"Pillow {PIL.__version__}")
FINE = 16
MASK = 4
STEP = FINE // MASK


def font(size):
    f = ImageFont.truetype(font_path, size * FINE)
    f.set_variation_by_axes([600])
    return f


def ink(f, text):
    probe = Image.new("L", (int(f.size * 3), int(f.size * 3)))
    ImageDraw.Draw(probe).text((f.size, f.size * 2), text, font=f, fill=255, anchor="ls")
    left, top, right, bottom = probe.getbbox()
    return (left - f.size) / FINE, (f.size * 2 - top) / FINE, (right - left) / FINE


def save(name, fine, x, y):
    small = fine.resize((fine.width // STEP, fine.height // STEP), Image.BOX)
    box = small.getbbox()
    small = small.crop(box)
    x, y = x + box[0], y + box[1]
    with open(f"{out}/{name}.pgm", "wb") as file:
        file.write(f"P5\n# at {x} {y}\n{small.width} {small.height}\n255\n".encode())
        file.write(small.tobytes())


def canvas(width, height):
    image = Image.new("L", (math.ceil(width * FINE), math.ceil(height * FINE)))
    return image, ImageDraw.Draw(image)


def fine_rect(draw, x, y, w, h):
    draw.rectangle([round(x * FINE), round(y * FINE), round((x + w) * FINE) - 1, round((y + h) * FINE) - 1], fill=255)


S, ICON_X, ICON_Y = 72, 124, 47
glyph_font = font(S * 0.9)
left, ascent, width = ink(glyph_font, "n")
bar_w, bar_h, gap = 0.23 * S, max(1, 0.075 * S), 0.06 * S
x0 = (S - (width + gap + bar_w)) / 2
baseline = (S + ascent) / 2

tile, draw = canvas(S, S)
draw.rounded_rectangle([0, 0, S * FINE - 1, S * FINE - 1], radius=0.22 * S * FINE, fill=255)
save("tile", tile, ICON_X * MASK, ICON_Y * MASK)

glyph, draw = canvas(S, S)
draw.text(((x0 - left) * FINE, baseline * FINE), "n", font=glyph_font, fill=255, anchor="ls")
save("glyph", glyph, ICON_X * MASK, ICON_Y * MASK)

bar, draw = canvas(S, S)
fine_rect(draw, x0 + width + gap, baseline - bar_h, bar_w, bar_h)
save("bar", bar, ICON_X * MASK, ICON_Y * MASK)

WORD, SIZE, TRACK, CENTRE, LINE_TOP = "navo", 22, -0.01 * 22, 160, 131
word_font = font(SIZE)
up, down = (value / FINE for value in word_font.getmetrics())
word_baseline = LINE_TOP + (SIZE - (up + down)) / 2 + up
pens = [(word_font.getlength(WORD[: i + 1]) - word_font.getlength(ch)) / FINE + TRACK * i for i, ch in enumerate(WORD)]
total = word_font.getlength(WORD) / FINE + TRACK * len(WORD)
start = CENTRE - total / 2
left_q, top_q = math.floor(start * MASK), math.floor((LINE_TOP - SIZE) * MASK)
word, draw = canvas(total + 2 * SIZE, 3 * SIZE)
for pen, ch in zip(pens, WORD):
    x = (start + pen - left_q / MASK) * FINE
    y = (word_baseline - top_q / MASK) * FINE
    draw.text((x, y), ch, font=word_font, fill=255, anchor="ls")
save("wordmark", word, left_q, top_q)
PY

echo "Wrote $out"
