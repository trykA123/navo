import { prog, ease } from '../ease.js';
import { el, put, clamp01 } from '../dom.js';
import { appWindow, pane, sheet, setCursor, RIGHT_X } from '../chrome.js';
import { icon } from '../icons.js';
import { LEFT } from '../data.js';
import { THEMES } from '../data-roadmap.js';
import { cursorIndex } from './common.js';

const smooth = (x) => { const v = clamp01(x); return v * v * (3 - 2 * v); };
const LIST = [...THEMES, { id: 'harbor', name: 'Harbor', sw: ['oklch(18% 0.03 240)', 'oklch(74% 0.11 230)', 'oklch(80% 0.1 85)'] }];
const SEL = [[2.0, 1], [3.6, 2], [5.2, 3], [8.8, 2], [8.92, 1], [9.04, 0]];

function themeWindow(id) {
  const win = appWindow(`theme-${id}`);
  const left = pane(LEFT, { x: 12 });
  setCursor(left, 6, 1);
  const rows = LIST.map((th) => `<div class="thr"><span class="sws">${th.sw.map((c) => `<i style="background:${c}"></i>`).join('')}</span><span>${th.name}</span>${th.id === 'dark' ? '<span class="tbadge">Default</span>' : ''}</div>`).join('');
  const st = sheet('settings appearance', { x: RIGHT_X, w: 782 }, `
    <div class="sh-head"><div class="sh-icon">${icon('palette')}</div><div class="sh-name">Appearance</div><span class="sh-close kbd">Esc</span></div>
    <h3><span>Colour theme</span><span>live preview</span></h3><div class="thlist"><div class="thsel"></div>${rows}</div>
    <div class="srow"><div><div class="sl">Follow Windows</div><div class="sd">Light by day, dark at night</div></div><span class="tog on"><i></i></span></div>
    <div class="srow"><div><div class="sl">Icon theme</div><div class="sd">File-type icons by extension</div></div><div class="seg"><span class="on">Navo</span><span>Lucide line</span></div></div>
    <div class="srow"><div><div class="sl">Density</div><div class="sd">Row height in the file lists</div></div><div class="seg"><span class="on">Compact</span><span>Comfortable</span></div></div>
    <div class="srow"><div><div class="sl">Theme files</div><div class="sd mono">themes\\*.toml · reloads when saved</div></div><span class="chip">Open folder</span></div>`);
  left.ring.style.opacity = '1';
  win.append(left.node, st);
  return { win, sel: st.querySelector('.thsel') };
}

export default {
  id: 'themes',
  dur: 12.6,
  captions: [[0.8, 8.4, 'Colour themes and icon themes, applied live.', 'Planned · beta'], [8.9, 11.6, 'Light and dark follow Windows.']],
  keys: [[0.5, 'Ctrl ,'], [2.0, 'Down'], [3.6, 'Down'], [5.2, 'Down'], [8.8, 'Up'], [8.92, 'Up'], [9.04, 'Up']],
  build() {
    const node = el('<div class="scene-root"></div>');
    const copies = THEMES.map((th) => themeWindow(th.id));
    copies.forEach((c) => node.append(c.win));
    return { node, copies };
  },
  render(r, t) {
    const sel = cursorIndex(t, SEL, 0);
    r.copies.forEach((c) => { c.sel.style.transform = `translateY(${(sel * 44).toFixed(2)}px)`; });
    const a = [
      1,
      smooth((t - 2.05) / 0.7) * (t < 8.6 ? 1 : 0),
      smooth((t - 3.65) / 0.7) * (t < 8.6 ? 1 : 0),
      smooth((t - 5.25) / 2.4) * (1 - smooth((t - 8.85) / 2.2)),
    ];
    r.copies.forEach((c, i) => put(c.win, a[i], ''));
    const sh = prog(t, 0.55, 0.5, ease.drawer);
    r.copies.forEach((c) => {
      const s = c.win.querySelector('.sheet');
      put(s, sh > 0.001 ? 1 : 0, `translateX(${((1 - sh) * 830).toFixed(2)}px)`);
    });
  },
};
