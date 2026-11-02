import { prog, ease } from '../ease.js';
import { el, put, text, html } from '../dom.js';
import { appWindow, pane, sheet, setCursor, RIGHT_X } from '../chrome.js';
import { noticeStrip } from '../parts.js';
import { icon } from '../icons.js';
import { LEFT, RIGHT } from '../data.js';
import { DELETED } from '../data-roadmap.js';
import { ROW } from '../script.js';
import { active, sheetSlide, modal, cursorIndex } from './common.js';

const T = { open: 0.75, restore: 2.95, close: 5.2, card: 5.75, ok: 9.1 };
const TRACE = 14;

export default {
  id: 'safety',
  dur: 9.6,
  captions: [[0.9, 5.0, 'Deleted by mistake? Bring it back for up to 7 days.', 'Planned · beta'], [5.9, 9.2, 'A crash-safe journal finishes or rolls back every job.']],
  keys: [[0.7, 'Alt Del'], [1.9, 'Down'], [2.2, 'Down'], [2.9, 'Enter'], [5.15, 'Esc'], [9.05, 'Enter']],
  build() {
    const node = el('<div class="scene-root"></div>');
    const win = appWindow();
    const left = pane(LEFT, { x: 12 });
    const right = pane({ ...RIGHT, rows: RIGHT.rows.filter((row) => !row.copy) }, { x: RIGHT_X });
    let n = 0;
    const groups = DELETED.map((g) => `<h3><span>${g.group}</span><span>${g.items.length}</span></h3>${g.items.map((it) => `<div class="drow" data-i="${n++}">
      <span class="k-${it.kind}">${icon(it.kind)}</span><span class="dn">${it.name}</span><span class="df mono">${it.from}</span><span class="ds mono">${it.size}</span><span class="dt mono">${it.at}</span></div>`).join('')}`).join('');
    const sh = sheet('delsheet', { x: RIGHT_X, w: 782 }, `
      <div class="sh-head"><div class="sh-icon trash">${icon('trash')}</div><div><div class="sh-name">Recently deleted</div><div class="sh-path">Only what Navo deleted. Kept for 7 days, then purged.</div></div><span class="sh-close kbd">Esc</span></div>
      <div class="cap"><div class="capbar"><i></i></div><span class="mono">1.4 GB of 20 GB on D:</span></div>
      <div class="dlist"><div class="dsel"></div>${groups}</div>
      <div class="dkeys"><span><span class="kbd">Enter</span> restore</span><span><span class="kbd">Shift+Enter</span> restore to…</span><span><span class="kbd">Del</span> purge now</span></div>`);
    const strip = noticeStrip();
    const scrim = el('<div class="scrim"></div>');
    const card = el(`<div class="dialog recover" style="left:470px;top:180px">
      <div class="rh"><div class="sh-icon ok">${icon('shield')}</div><div><h2>Recovered after an unexpected exit</h2><p>The journal checked 2 unfinished jobs. Nothing was lost.</p></div></div>
      <div class="jl"><div>${icon('check')}<span>Copy 3 files to E:\\Archive\\releases</span><b>finished and verified</b></div>
        <div>${icon('undo')}<span>Move 2 files to \\\\nas-01\\builds</span><b>rolled back</b></div></div>
      <div class="btns"><div class="btn">Show journal</div><div class="btn primary">OK <span class="kbd">Enter</span></div></div></div>`);
    win.append(left.node, right.node, sh, strip.node, scrim, card);
    node.append(win);
    const drows = [...sh.querySelectorAll('.drow')];
    return { node, left, right, sh, strip, scrim, card, drows, dsel: sh.querySelector('.dsel'), cap: sh.querySelector('.capbar i') };
  },
  render(r, t) {
    active(r.left, r.right, t < T.open || t > T.close ? 'left' : 'none');
    setCursor(r.left, 2, 1); setCursor(r.right, 0, 0.4);
    const shown = sheetSlide(r.sh, t, T.open, T.close);
    r.cap.style.transform = `scaleX(${(0.07 * prog(t, T.open + 0.3, 0.8)).toFixed(4)})`;
    const back = prog(t, T.restore + 0.15, 0.4, ease.inOut);
    const trace = r.drows[2];
    put(trace, 1 - back, `translateX(${back * -24}px)`);
    r.drows.forEach((d, i) => {
      if (i > 2) d.style.transform = `translateY(${(-back * 40).toFixed(2)}px)`;
      put(d, i === 2 ? 1 - back : prog(t, T.open + 0.2 + i * 0.04, 0.35), d.style.transform);
    });
    const sel = cursorIndex(t, [[1.9, 1], [2.2, 2]]);
    r.dsel.style.transform = `translateY(${(sel * 40).toFixed(2)}px)`;
    r.dsel.style.opacity = shown > 0.5 && t < T.restore + 0.2 ? '1' : '0';

    const ins = prog(t, T.restore + 0.3, 0.4, ease.inOut);
    r.left.rows.forEach((row, i) => {
      if (i < TRACE) return;
      if (i === TRACE) { put(row.el, ins, `translateY(${i * ROW}px)`); row.el.classList.toggle('restored', ins > 0 && t < T.restore + 2.4); return; }
      row.el.style.transform = `translateY(${(i - 1 + ins) * ROW}px)`;
    });
    text(r.left.foot, ins > 0.5 ? '10 files, 5 folders · 71.2 MB' : '9 files, 5 folders · 29.2 MB');

    const s = r.strip;
    const on = prog(t, T.restore + 0.35, 0.4, ease.drawer) * (1 - prog(t, T.card - 0.3, 0.3));
    put(s.node, on > 0.001 ? 1 : 0, `translateY(${(1 - on) * 72}px)`);
    html(s.icon, icon('undo'));
    text(s.label, 'Restored trace_bench.blf to D:\\Work\\aurora');
    html(s.hint, '<span class="kbd">Ctrl+Z</span> undo');

    modal(r.card, t, T.card, T.ok, r.scrim);
  },
};
