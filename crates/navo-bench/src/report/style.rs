/// Design tokens as OKLCH custom properties; light by default, dark by colour scheme.
pub const CSS: &str = "\
:root{color-scheme:light dark;\
--bg:oklch(0.965 0.005 250);--surface:oklch(1 0 0);--raised:oklch(0.935 0.008 250);\
--line:oklch(0.85 0.01 250);--text:oklch(0.22 0.015 250);--muted:oklch(0.42 0.02 250);\
--pass:oklch(0.42 0.11 150);--pass-bg:oklch(0.94 0.04 150);\
--fail:oklch(0.45 0.17 27);--fail-bg:oklch(0.94 0.035 27);--bar:oklch(0.52 0.12 250)}\
@media (prefers-color-scheme:dark){:root{\
--bg:oklch(0.19 0.01 250);--surface:oklch(0.23 0.012 250);--raised:oklch(0.27 0.014 250);\
--line:oklch(0.35 0.014 250);--text:oklch(0.93 0.008 250);--muted:oklch(0.76 0.02 250);\
--pass:oklch(0.82 0.13 150);--pass-bg:oklch(0.3 0.05 150);\
--fail:oklch(0.8 0.12 27);--fail-bg:oklch(0.3 0.06 27);--bar:oklch(0.7 0.11 250)}}\
*{box-sizing:border-box}\
body{margin:0;padding:1rem;background:var(--bg);color:var(--text);\
font:16px/1.5 system-ui,-apple-system,'Segoe UI',sans-serif}\
main{max-width:72rem;margin:0 auto}\
h1{font-size:1.5rem;margin:0 0 .25rem}h2{font-size:1.15rem;margin:0 0 .75rem}\
h3{font-size:1rem;margin:1rem 0 .5rem}\
section{background:var(--surface);border:1px solid var(--line);border-radius:8px;\
padding:1rem;margin:1rem 0}\
p{margin:.25rem 0 .75rem}.muted{color:var(--muted)}\
.scroll{overflow-x:auto;max-width:100%}\
.scroll:focus-visible{outline:2px solid var(--bar);outline-offset:2px}\
table{border-collapse:collapse;width:100%}\
.wide{min-width:36rem}.wide th:first-child,.wide td:first-child{min-width:10rem}\
th,td{text-align:left;padding:.4rem .6rem;border-bottom:1px solid var(--line);vertical-align:top}\
th{background:var(--raised);font-weight:600}\
.n{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}\
.badge{display:inline-block;padding:0 .5rem;border-radius:4px;font-weight:600;\
background:var(--raised);color:var(--text)}\
.pass{background:var(--pass-bg);color:var(--pass)}\
.fail{background:var(--fail-bg);color:var(--fail)}\
.tiles{display:grid;grid-template-columns:repeat(auto-fit,minmax(9rem,1fr));gap:.75rem}\
.tile{background:var(--raised);border-radius:6px;padding:.5rem .75rem}\
.tile b{display:block;font-size:1.5rem}\
.hist{display:grid;grid-template-columns:7rem 1fr 6rem;gap:.25rem .75rem;align-items:center}\
.track{background:var(--raised);height:.75rem;border-radius:3px;overflow:hidden}\
.track i{display:block;height:100%;background:var(--bar)}\
@media (max-width:30rem){.hist{grid-template-columns:5.5rem 1fr 4.5rem}body{padding:.5rem}}";
