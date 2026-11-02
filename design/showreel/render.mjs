import { chromium } from 'playwright-core';
import { readFile, mkdir, rm } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const fonts = path.resolve(here, '../../crates/navo-ui/fonts');
const framesDir = path.join(here, 'frames');
const args = Object.fromEntries(process.argv.slice(2).map((a) => a.replace(/^--/, '').split('=')));
const workers = Number(args.workers ?? 3);
const draft = 'draft' in args;
const step = draft ? 2 : 1;
const ext = draft ? 'jpg' : 'png';
const chromiumPath = args.chromium ?? process.env.CHROMIUM ?? '/usr/bin/chromium';
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.ttf': 'font/ttf' };

async function openPage(browser) {
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: draft ? 0.5 : 1 });
  await page.route('http://reel.local/**', async (route) => {
    const url = new URL(route.request().url());
    const file = url.pathname.startsWith('/fonts/')
      ? path.join(fonts, path.basename(url.pathname))
      : path.join(here, path.normalize(url.pathname));
    if (!file.startsWith(here) && !file.startsWith(fonts)) return route.abort();
    try {
      await route.fulfill({ body: await readFile(file), contentType: TYPES[path.extname(file)] ?? 'application/octet-stream' });
    } catch {
      await route.fulfill({ status: 404, body: 'not found' });
    }
  });
  page.on('pageerror', (e) => console.error('page error:', e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('console:', m.text()); });
  await page.goto('http://reel.local/index.html');
  await page.waitForFunction(() => window.reel?.ready === true);
  const ok = await page.evaluate(() => document.fonts.check('600 16px Archivo') && document.fonts.check('400 14px "JetBrains Mono"'));
  if (!ok) throw new Error('fonts did not load');
  return page;
}

async function shoot(page, n, file) {
  await page.evaluate((frame) => window.reel.frame(frame), n);
  await page.screenshot(file.endsWith('.jpg') ? { path: file, type: 'jpeg', quality: 90 } : { path: file, type: 'png' });
}

function run(cmd, cmdArgs) {
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, cmdArgs, { stdio: 'inherit' });
    child.on('exit', (code) => (code === 0 ? resolve() : reject(new Error(`${cmd} exited ${code}`))));
  });
}

const browser = await chromium.launch({
  executablePath: chromiumPath,
  args: ['--force-color-profile=srgb', '--font-render-hinting=none', '--disable-lcd-text', '--hide-scrollbars'],
});

if (args.stills) {
  const dir = path.join(framesDir, 'stills');
  await mkdir(dir, { recursive: true });
  const page = await openPage(browser);
  for (const t of args.stills.split(',')) {
    await shoot(page, Math.round(Number(t) * 60), path.join(dir, `t${Number(t).toFixed(2)}.png`));
  }
  await browser.close();
  process.exit(0);
}

await rm(framesDir, { recursive: true, force: true });
await mkdir(framesDir, { recursive: true });
const probe = await openPage(browser);
const total = await probe.evaluate(() => window.reel.frames);
await probe.close();
const count = Math.ceil(total / step);
const per = Math.ceil(count / workers);
const started = Date.now();
await Promise.all(Array.from({ length: workers }, async (_, w) => {
  const page = await openPage(browser);
  for (let k = w * per; k < Math.min(count, (w + 1) * per); k++) {
    await shoot(page, k * step, path.join(framesDir, `${String(k).padStart(5, '0')}.${ext}`));
    if (k % 300 === 0) console.log(`frame ${k}/${count} after ${((Date.now() - started) / 1000).toFixed(0)} s`);
  }
}));
await browser.close();

const fps = String(60 / step);
const out = draft ? path.join(framesDir, 'draft.mp4') : path.join(here, 'navo-showreel.mp4');
await run('ffmpeg', [
  '-y', '-loglevel', 'error', '-framerate', fps, '-i', path.join(framesDir, `%05d.${ext}`),
  '-vf', 'scale=out_color_matrix=bt709:out_range=tv,format=yuv420p',
  '-c:v', 'libx264', '-preset', 'slow', '-crf', String(args.crf ?? 18), '-tune', 'animation', '-threads', '6',
  '-r', fps, '-g', String(2 * Number(fps)), '-colorspace', 'bt709', '-color_primaries', 'bt709', '-color_trc', 'bt709', '-color_range', 'tv',
  '-movflags', '+faststart', out,
]);
if (draft) {
  console.log(`draft: ${count} frames in ${((Date.now() - started) / 1000).toFixed(0)} s -> ${out}`);
  process.exit(0);
}
const poster = Math.round(Number(args.poster ?? 60) * 60);
await run('ffmpeg', ['-y', '-loglevel', 'error', '-i', path.join(framesDir, `${String(poster).padStart(5, '0')}.png`),
  '-vf', 'scale=1280:-1:flags=lanczos', path.join(here, 'poster.png')]);
console.log(`done: ${total} frames in ${((Date.now() - started) / 1000).toFixed(0)} s`);
