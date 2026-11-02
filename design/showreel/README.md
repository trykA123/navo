# Navo showreel

A 148-second product film that presents Navo as the roadmap plans it, from the alpha to 2.0. It is built from the feature specs, the design prototypes and the app's Design B dark tokens. It is not a screen recording, and every file name, folder, host and size in it is invented.

Much of the film shows features that are not built yet. Their captions carry a "Planned" tag with the phase (beta, 1.0 or 2.0), and the closing card says so too.

- `navo-showreel.mp4`: 1920×1080, 60 fps, H.264, 8,880 frames.
- `poster.png`: the still the main README shows (at 107.5 s, the four viewers side by side).

## Chapters

| Time | Chapter | Scenes |
| --- | --- | --- |
| 0:00 | Open | Logo and tagline |
| 0:05 | 01 Panes | Two panes and the folder preview; tiled panes; saved sets on Alt+1 to Alt+9; pinned folders |
| 0:34 | 02 Speed and search | Ctrl+K for commands and files; find by content with results as a pane; archives as folders |
| 0:51 | 03 Operations and safety | A planned, journaled copy; multi-rename with a live check and Ctrl+Z; recently deleted; crash recovery |
| 1:17 | 04 The drawer | Transfers, clipboard history and a terminal that follows the pane; folder and file compare |
| 1:39 | 05 Viewers | Rich ELF view; code with Oxidised iron colours and the View menu; decoded CAN traces; PDF with outline |
| 1:59 | 06 Plugins | Install with permissions in plain words; a plugin column; modules on and off |
| 2:09 | 07 Look and feel | Colour themes applied live; light and dark |
| 2:20 | Close | Logo, roadmap line |

## How it is made

`index.html` builds every scene once. `window.reel.frame(n)` then sets every property from the frame number alone, so a frame always looks the same however it is reached. There are no CSS transitions or timers.

Each scene in `src/scenes/` exports its length, captions, key presses, a `build()` and a `render(refs, t)` that takes the time inside the scene. `src/timeline.js` lines the scenes up and joins them with a soft cut: a 0.8 s cross-fade with a light blur. `render.mjs` opens the page in headless Chromium, screenshots each frame to `frames/`, and encodes the frames with ffmpeg. The fonts come from `crates/navo-ui/fonts`.

| File | Holds |
| --- | --- |
| `src/timeline.js` | Scene order, chapters, the soft cuts |
| `src/scenes/*.js` | One file per scene: timing, captions, keys, DOM and per-frame state |
| `src/chrome.js`, `src/parts.js` | The window, panes, sheets and the reused parts (palette, folder preview, ELF view, job strip) |
| `src/overlay.js` | Captions, "Planned" tags, chapter labels and key caps |
| `src/data.js`, `src/data-roadmap.js` | The invented file lists and viewer data |
| `src/reel.css`, `src/scenes.css` | Tokens, layout and the four colour themes |
| `check-brightness.sh` | Largest change in average brightness between frames |

## Rebuild

You need Node, ffmpeg and Chromium.

```sh
cd design/showreel
npm ci --ignore-scripts
taskset -c 0-5 nice -n 10 node render.mjs --draft      # 960×540 at 30 fps, frames/draft.mp4
taskset -c 0-5 nice -n 10 node render.mjs --workers=3  # the final film and poster
./check-brightness.sh                                  # must stay under 3/255 per frame
```

`CHROMIUM=/path/to/chromium` picks another browser. `--stills=3,12.5,28` writes only those seconds to `frames/stills/`, which is quicker when you adjust a scene. `--poster=107.5` picks the poster second. To preview a moment in a browser, serve this folder with the fonts at `/fonts/` and open `index.html?t=12.5`.

`frames/` and `node_modules/` are ignored by git.
