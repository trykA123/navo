const d = (date) => `2026-${date}`;

export const LOGS = {
  tabs: ['logs'], crumbs: ['D:', 'Work', 'aurora', 'logs'], compact: true,
  foot: '7 files · 186 MB', free: '',
  rows: [
    { name: '..', kind: 'up' },
    { name: 'bench_0921', ext: 'blf', kind: 'auto', size: '42.0 MB', date: d('09-21 16:44') },
    { name: 'bench_1008', ext: 'blf', kind: 'auto', size: '61.3 MB', date: d('10-08 16:44') },
    { name: 'can_bus_a', ext: 'trc', kind: 'auto', size: '18.2 MB', date: d('10-14 11:02') },
    { name: 'flash_run', ext: 'log', kind: 'doc', size: '640 KB', date: d('10-16 09:41') },
    { name: 'hil_night', ext: 'trc', kind: 'auto', size: '52.8 MB', date: d('10-15 23:58') },
    { name: 'startup', ext: 'log', kind: 'doc', size: '96 KB', date: d('10-16 09:40') },
    { name: 'watchdog', ext: 'log', kind: 'doc', size: '12 KB', date: d('10-12 07:15') },
  ],
};

export const PHOTOS = {
  tabs: [{ label: '2026-10', icon: 'dir', pinned: true }, 'Exports'], crumbs: ['P:', 'Photos', '2026-10'], drive: 'P: Photos', compact: false,
  foot: '8 files · 41.6 MB', free: '3.1 TB free',
  rows: [
    { name: '..', kind: 'up' },
    { name: 'IMG_4410', ext: 'jpg', kind: 'image', size: '6.2 MB', date: d('10-02 09:14'), x: '4032 × 3024' },
    { name: 'IMG_4411', ext: 'jpg', kind: 'image', size: '6.0 MB', date: d('10-02 09:15'), x: '4032 × 3024' },
    { name: 'IMG_4412', ext: 'jpg', kind: 'image', size: '5.8 MB', date: d('10-02 09:15'), x: '3024 × 4032' },
    { name: 'IMG_4418', ext: 'jpg', kind: 'image', size: '4.9 MB', date: d('10-03 17:40'), x: '4032 × 3024' },
    { name: 'IMG_4419', ext: 'jpg', kind: 'image', size: '5.1 MB', date: d('10-03 17:41'), x: '4032 × 3024' },
    { name: 'IMG_4420', ext: 'heic', kind: 'image', size: '2.1 MB', date: d('10-04 18:22'), x: '4284 × 5712' },
    { name: 'IMG_4425', ext: 'jpg', kind: 'image', size: '6.4 MB', date: d('10-05 10:02'), x: '4032 × 3024' },
    { name: 'IMG_4431', ext: 'jpg', kind: 'image', size: '5.1 MB', date: d('10-06 12:30'), x: '4032 × 3024' },
  ],
};

export const EXPORTS = {
  tabs: ['Exports', 'web'], crumbs: ['E:', 'Exports', 'harbour-walk'], drive: 'E: Archive', compact: false,
  foot: '3 files, 1 folder · 9.8 MB', free: '1.42 TB free',
  rows: [
    { name: '..', kind: 'up' },
    { name: 'web', kind: 'dir', date: d('10-07 20:10') },
    { name: 'cover', ext: 'jpg', kind: 'image', size: '2.4 MB', date: d('10-07 20:02') },
    { name: 'harbour-walk', ext: 'pdf', kind: 'pdf', size: '6.9 MB', date: d('10-07 20:12') },
    { name: 'captions', ext: 'txt', kind: 'doc', size: '3 KB', date: d('10-07 20:15') },
  ],
};

export const NAS = {
  tabs: ['builds', 'Downloads'], crumbs: ['\\\\nas-01', 'builds', 'aurora'], crumbIcon: 'net', drive: '\\\\nas-01 builds', driveIcon: 'net',
  foot: '2 files, 4 folders · 31.0 MB', free: '6.8 TB free',
  rows: [
    { name: '..', kind: 'up' },
    { name: 'nightly', kind: 'net', date: d('10-16 03:00') },
    { name: 'release-0.8', kind: 'net', date: d('09-21 12:40') },
    { name: 'release-0.9', kind: 'net', date: d('10-15 07:30') },
    { name: 'tools', kind: 'net', date: d('10-01 08:12') },
    { name: 'build-index', ext: 'json', kind: 'code', size: '86 KB', date: d('10-16 03:04') },
    { name: 'aurora-0.9-full', ext: 'zip', kind: 'archive', size: '30.9 MB', date: d('10-15 07:31') },
  ],
};

export const SETS = [
  { key: '1', name: 'Firmware', sub: 'aurora · logs · photos', layout: 'quad' },
  { key: '2', name: 'Photos', sub: '2026-10 · Exports', layout: 'two' },
  { key: '3', name: 'Release', sub: 'builds · releases · notes', layout: 'three' },
  { key: '4', name: 'Admin', sub: 'nas-01 · scripts', layout: 'main' },
];

export const PINS = [
  { key: 'Ctrl+1', name: 'aurora', path: 'D:\\Work\\aurora', icon: 'dir' },
  { key: 'Ctrl+2', name: 'releases', path: 'E:\\Archive\\releases', icon: 'dir' },
  { key: 'Ctrl+3', name: 'Photos 2026', path: 'P:\\Photos', icon: 'dir' },
  { key: 'Ctrl+4', name: 'builds', path: '\\\\nas-01\\builds\\aurora', icon: 'net' },
  { key: '', name: 'CHANGES.md', path: 'Pinned file', icon: 'doc' },
];

export const FOUND = {
  tabs: [{ label: 'CAN_TIMEOUT', icon: 'search' }, 'releases'], crumbs: ['Search results', 'CAN_TIMEOUT in *.c *.h'], crumbIcon: 'search', sizeTitle: 'Hits',
  foot: '9 files in 5 folders · 23 hits', free: 'search finished in 0.4 s',
  rows: [
    { name: 'can_if', ext: 'c', kind: 'code', path: 'src\\can', size: '6', date: d('10-16 09:12') },
    { name: 'can_if', ext: 'h', kind: 'code', path: 'src\\can', size: '2', date: d('10-16 09:12') },
    { name: 'can_tp', ext: 'c', kind: 'code', path: 'src\\can', size: '4', date: d('10-15 18:40') },
    { name: 'com_cfg', ext: 'h', kind: 'code', path: 'src\\config', size: '3', date: d('10-14 10:31') },
    { name: 'diag_dtc', ext: 'c', kind: 'code', path: 'src\\diag', size: '2', date: d('10-12 17:03') },
    { name: 'gateway', ext: 'c', kind: 'code', path: 'src\\app', size: '2', date: d('10-16 08:55') },
    { name: 'nm_ctrl', ext: 'c', kind: 'code', path: 'src\\can', size: '1', date: d('10-09 10:31') },
    { name: 'test_can_if', ext: 'c', kind: 'code', path: 'tests\\unit', size: '2', date: d('10-16 09:38') },
    { name: 'timeouts', ext: 'h', kind: 'code', path: 'src\\config', size: '1', date: d('10-02 14:06') },
  ],
};

export const ZIP = {
  tabs: ['aurora', 'Downloads'], crumbs: ['D:', 'Work', 'aurora', 'release-0.9.zip'], drive: 'D: Work',
  foot: 'Archive · 4 files, 2 folders · 18.4 MB packed', free: 'read as a folder',
  rows: [
    { name: '..', kind: 'up' },
    { name: 'bin', kind: 'dir', date: d('10-15 07:28') },
    { name: 'docs', kind: 'dir', date: d('10-15 07:28') },
    { name: 'aurora_fw', ext: 'hex', kind: 'code', size: '6.8 MB', date: d('10-15 07:26') },
    { name: 'aurora_fw', ext: 'elf', kind: 'code', size: '2.4 MB', date: d('10-15 07:26') },
    { name: 'checksums', ext: 'txt', kind: 'doc', size: '2 KB', date: d('10-15 07:29') },
    { name: 'RELEASE-NOTES', ext: 'md', kind: 'doc', size: '9 KB', date: d('10-15 07:20') },
  ],
};

export const RENAME = [
  ['IMG_4410.jpg', '2026-10-02', '001'],
  ['IMG_4411.jpg', '2026-10-02', '002'],
  ['IMG_4412.jpg', '2026-10-02', '003'],
  ['IMG_4418.jpg', '2026-10-03', '004'],
  ['IMG_4419.jpg', '2026-10-03', '005'],
  ['IMG_4420.heic', '2026-10-04', '006'],
  ['IMG_4425.jpg', '2026-10-05', '007'],
  ['IMG_4431.jpg', '2026-10-06', '008'],
];

export const DELETED = [
  { group: 'Today', items: [
    { name: 'build-cache', kind: 'dir', from: 'D:\\Work\\aurora', size: '3,904 items', at: '14:02' },
    { name: 'old-notes.md', kind: 'doc', from: 'D:\\Work\\aurora\\docs', size: '12 KB', at: '11:47' },
    { name: 'trace_bench.blf', kind: 'auto', from: 'D:\\Work\\aurora', size: '42.0 MB', at: '10:15' },
  ] },
  { group: 'Yesterday', items: [
    { name: 'IMG_4399.jpg', kind: 'image', from: 'P:\\Photos\\2026-10', size: '5.6 MB', at: '19:30' },
    { name: 'linker-old.lsl', kind: 'code', from: 'D:\\Work\\aurora', size: '8 KB', at: '16:02' },
  ] },
  { group: '3 days ago', items: [
    { name: 'release-0.7.zip', kind: 'archive', from: 'E:\\Archive', size: '16.1 MB', at: '09:12' },
  ] },
];

export const JOBS = [
  { icon: 'copy', label: 'Copy 1,204 files to \\\\nas-01\\builds\\nightly', mode: 'Parallel 4 · SHA-256', size: 2.8 },
  { icon: 'transfer', label: 'Move 18 photos to E:\\Exports\\harbour-walk', mode: 'Auto · fast hash', size: 0.11 },
  { icon: 'check', label: 'Verify 3 files on E:\\Archive\\releases', mode: 'SHA-256', size: 0.01 },
];

export const CLIPS = [
  { icon: 'dir', kind: 'Path', text: 'D:\\Work\\aurora\\build\\release', meta: 'copied 2 min ago', k: 'dir' },
  { icon: 'code', kind: 'Text', text: '#define CAN_TIMEOUT_MS   250u', meta: 'from can_if.h · 6 min ago', k: 'code' },
  { icon: 'image', kind: 'Image', text: 'flash_layout.png · 1280 × 720', meta: '14 min ago', k: 'image' },
  { icon: 'copy', kind: 'Files', text: '3 files from E:\\Archive\\releases', meta: '22 min ago', k: 'cmd' },
  { icon: 'net', kind: 'Path', text: '\\\\nas-01\\builds\\aurora\\nightly', meta: '1 h ago', k: 'dir' },
];

export const TERM = [
  ['p', 'D:\\Work\\aurora>', 'make release BOARD=tc4'],
  ['o', '  CC   src/can/can_if.c'],
  ['o', '  CC   src/can/can_tp.c'],
  ['o', '  CC   src/diag/diag_dtc.c'],
  ['o', '  LD   build/release/aurora_fw.elf'],
  ['g', '  OK   aurora_fw.elf  2.41 MB flash · 171 KB RAM'],
  ['p', 'D:\\Work\\aurora>', ''],
];

export const COMPARE = [
  ['dir', 'docs', 'same', 0], ['dir', 'src', 'diff', 0],
  ['dir', 'can', 'diff', 1],
  ['code', 'can_if.c', 'diff', 2, '3.1 KB', '3.5 KB', '+14 −9'],
  ['code', 'can_tp.c', 'same', 2, '5.0 KB', '5.0 KB'],
  ['code', 'can_gw.c', 'right', 2, '', '6.7 KB'],
  ['code', 'legacy_bus.c', 'left', 2, '5.3 KB', ''],
  ['dir', 'diag', 'diff', 1],
  ['code', 'diag_dtc.c', 'diff', 2, '4.8 KB', '5.0 KB', '+12 −6'],
  ['code', 'diag_io.c', 'same', 2, '2.2 KB', '2.2 KB'],
  ['code', 'main.c', 'diff', 1, '2.0 KB', '2.1 KB', '+2 −0'],
  ['dir', 'tests', 'same', 0],
  ['code', 'linker.lsl', 'diff', 0, '8.0 KB', '8.1 KB', '+3 −1'],
  ['doc', 'CHANGES.md', 'diff', 0, '12 KB', '14 KB', '+41 −2'],
  ['doc', 'README.md', 'same', 0, '4.0 KB', '4.0 KB'],
];

export const DIFF = [
  [41, 41, 'same', 'static uint32_t can_last_rx_ms;', null],
  [42, 42, 'same', '', null],
  [43, 43, 'chg', 'void can_if_poll(uint32_t now_ms)', 'void can_if_poll(uint32_t now_ms, can_bus_t bus)', ['', ', can_bus_t bus']],
  [44, 44, 'same', '{', null],
  [45, 45, 'chg', '    if (now_ms - can_last_rx_ms > 100u) {', '    if (now_ms - can_last_rx_ms > CAN_TIMEOUT_MS) {', ['100u', 'CAN_TIMEOUT_MS']],
  [46, 46, 'same', '        can_if_report_timeout();', null],
  [47, null, 'del', '        can_if_reset();', null],
  [null, 47, 'add', null, '        can_if_reset_bus(bus);'],
  [null, 48, 'add', null, '        diag_dtc_set(DTC_CAN_TIMEOUT);'],
  [48, 49, 'same', '    }', null],
  [49, 50, 'same', '}', null],
  [50, 51, 'same', '', null],
  [51, 52, 'chg', '/* poll every 10 ms */', '/* poll every 5 ms on bus A, 10 ms on bus B */', ['10 ms', '5 ms on bus A, 10 ms on bus B']],
];

export const CAN = [
  ['0.000120', '0x0C4', 'EngineData', '8', '1F 40 03 E8 00 64 00 00'],
  ['0.010133', '0x1A0', 'BrakeStatus', '4', '00 12 00 01'],
  ['0.010410', '0x0C4', 'EngineData', '8', '1F 52 03 EA 00 64 00 00'],
  ['0.020118', '0x2F1', 'GatewayDiag', '8', '02 10 03 00 00 00 00 00'],
  ['0.020402', '0x0C4', 'EngineData', '8', '1F 68 03 F0 00 65 00 00'],
  ['0.030127', '0x1A0', 'BrakeStatus', '4', '00 14 00 01'],
  ['0.030399', '0x0C4', 'EngineData', '8', '1F 7C 03 F4 00 65 00 00'],
  ['0.040131', '0x3B2', 'BodyLights', '2', '0C 01'],
  ['0.040405', '0x0C4', 'EngineData', '8', '1F 90 03 FA 00 66 00 00'],
  ['0.050122', '0x1A0', 'BrakeStatus', '4', '00 15 00 00'],
  ['0.050398', '0x0C4', 'EngineData', '8', '1F A4 04 00 00 66 00 00'],
  ['0.060119', '0x2F1', 'GatewayDiag', '8', '02 10 03 00 00 00 00 00'],
  ['0.060402', '0x0C4', 'EngineData', '8', '1F B8 04 06 00 67 00 00'],
  ['0.070125', '0x1A0', 'BrakeStatus', '4', '00 15 00 00'],
  ['0.070401', '0x0C4', 'EngineData', '8', '1F CA 04 0C 00 67 00 00'],
  ['0.080130', '0x3B2', 'BodyLights', '2', '0C 00'],
  ['0.080399', '0x0C4', 'EngineData', '8', '1F D4 04 10 00 68 00 00'],
  ['0.090122', '0x1A0', 'BrakeStatus', '4', '00 16 00 00'],
  ['0.090404', '0x0C4', 'EngineData', '8', '1F A0 04 14 00 68 00 00'],
];

export const SIGNALS = [
  ['EngineSpeed', '8,100', 'rpm'],
  ['ThrottlePos', '40.2', '%'],
  ['CoolantTemp', '92', '°C'],
  ['TorqueReq', '101', 'Nm'],
];

export const PDF_OUTLINE = ['1 Overview', '2 Pin description', '3 CAN interface', '3.1 Timing', '3.2 Wake-up', '4 Electrical data', '5 Package'];

export const MODULES = [
  { icon: 'git', name: 'Git status', note: 'Branch and changes as a column', on: true },
  { icon: 'clipboard', name: 'Clipboard history', note: 'Last 200 items, kept on this PC', on: true },
  { icon: 'cpu', name: 'Disk usage', note: 'Folder sizes as a treemap', on: true },
  { icon: 'search', name: 'Everything search', note: 'Use Everything when it runs', on: false },
  { icon: 'dir', name: 'Weekly Downloads folders', note: 'Plugin · sorts Downloads by week', on: true, plugin: true },
  { icon: 'image', name: 'Image columns', note: 'Plugin · adds Dimensions', on: true, plugin: true, fresh: true },
];

export const THEMES = [
  { id: 'dark', name: 'Navo Dark', sw: ['oklch(16% 0.008 260)', 'oklch(72% 0.13 262)', 'oklch(80% 0.11 85)'] },
  { id: 'graphite', name: 'Graphite', sw: ['oklch(17% 0.004 60)', 'oklch(72% 0.12 50)', 'oklch(80% 0.09 85)'] },
  { id: 'patina', name: 'Patina', sw: ['oklch(17% 0.02 200)', 'oklch(74% 0.1 185)', 'oklch(80% 0.1 85)'] },
  { id: 'light', name: 'Navo Light', sw: ['oklch(98.5% 0.004 260)', 'oklch(52% 0.17 262)', 'oklch(62% 0.13 75)'] },
];
