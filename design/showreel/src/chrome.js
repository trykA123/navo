import { icon, logo } from './icons.js';
import { el } from './dom.js';
import { WIN, ROW } from './script.js';

export const PANE_H = WIN.h - 76;
export const LEFT_X = 12;
export const RIGHT_X = 806;
export const PANE_W = 782;
export const PANE_TOP = 64;

export function appWindow(extraClass = '') {
  const node = el(`<div class="window ${extraClass}" style="left:${WIN.x}px;top:${WIN.y}px;width:${WIN.w}px;height:${WIN.h}px">
    <header class="titlebar"><div class="app">${logo('mini-logo')}Navo</div>
      <div class="search">${icon('search')}<span class="grow">Search files, folders and commands</span><span class="kbd">Ctrl K</span></div>
      <div class="winctl">${icon('minus')}${icon('square')}${icon('x')}</div></header></div>`);
  return node;
}

export function rowHtml(r) {
  const kind = r.kind;
  const isDir = kind === 'dir' || kind === 'net';
  const name = isDir ? `[${r.name}]` : r.name;
  const size = isDir ? (r.size ?? '&lt;DIR&gt;') : r.size ?? '';
  return `<div class="row ${isDir ? 'dir' : ''}">
    <span class="k-${kind}">${icon(kind)}</span>
    <span class="name"><span class="nm">${name}</span>${r.path ? `<span class="path">${r.path}</span>` : ''}</span><span class="ext">${r.ext ?? ''}</span>
    <span class="xc">${r.x ?? ''}</span><span class="size">${size}</span><span class="date">${r.date ?? ''}</span></div>`;
}

function tabHtml(tab, on) {
  const label = typeof tab === 'string' ? tab : tab.label;
  const ic = typeof tab === 'string' ? 'dir' : tab.icon ?? 'dir';
  const pin = typeof tab === 'object' && tab.pinned ? `<span class="pin">${icon('pin')}</span>` : '';
  return `<div class="tab ${on ? 'on' : ''}">${pin}${icon(ic)}${label}</div>`;
}

export function crumbsHtml(crumbs, crumbIcon) {
  return (crumbIcon ? `<span class="k-${crumbIcon}">${icon(crumbIcon)}</span>` : '')
    + crumbs.map((c, i) => (i === crumbs.length - 1 ? `<b>${c}</b>` : `${c}${icon('chevron')}`)).join('');
}

export function pane(data, rect = {}) {
  const { x = LEFT_X, y = PANE_TOP, w = PANE_W, h = PANE_H } = rect;
  const tabs = (data.tabs ?? []).map((t, i) => tabHtml(t, i === 0)).join('');
  const drive = data.drive ? `<div class="drive">${icon(data.driveIcon ?? 'drive')}<b>${data.drive.split(' ')[0]}</b>${data.drive.split(' ').slice(1).join(' ')}</div>` : '';
  const x0 = data.extra ? `<span class="xc">${data.extra}</span>` : '<span class="xc"></span>';
  const node = el(`<section class="pane ${data.compact ? 'compact' : ''}" style="left:${x}px;top:${y}px;width:${w}px;height:${h}px">
    <div class="tabs">${tabs}<div class="tab add">${icon('plus')}</div>${drive}</div>
    <div class="crumbs">${crumbsHtml(data.crumbs, data.crumbIcon)}</div>
    <div class="head"><span></span><span>Name ↑</span><span>Ext</span>${x0}<span class="r">${data.sizeTitle ?? 'Size'}</span><span class="r">Date</span></div>
    <div class="list"><div class="cursor"></div></div>
    <div class="foot"><span class="fl">${data.foot ?? ''}</span><span class="fr">${data.free ?? ''}</span></div>
    <div class="ring"></div>
  </section>`);
  const list = node.querySelector('.list');
  const rows = data.rows.map((r, i) => {
    const row = el(rowHtml(r));
    row.style.transform = `translateY(${i * ROW}px)`;
    list.append(row);
    return { el: row, data: r };
  });
  return {
    node, list, rows, data,
    cursor: node.querySelector('.cursor'), ring: node.querySelector('.ring'),
    foot: node.querySelector('.fl'), crumbs: node.querySelector('.crumbs'), tabs: node.querySelector('.tabs'),
  };
}

export function setRect(p, r) {
  const s = p.node.style;
  s.left = `${r.x}px`; s.top = `${r.y}px`; s.width = `${Math.max(0, r.w)}px`; s.height = `${Math.max(0, r.h)}px`;
}

export function setCursor(p, index, o = 1) {
  p.cursor.style.opacity = o.toFixed(3);
  p.cursor.style.transform = `translateY(${(index * ROW).toFixed(2)}px)`;
}

export function sheet(cls, rect, inner) {
  return el(`<aside class="sheet ${cls}" style="left:${rect.x}px;top:${rect.y ?? PANE_TOP}px;width:${rect.w}px;height:${rect.h ?? PANE_H}px">${inner}</aside>`);
}

export function seeded(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 4294967296;
  };
}
