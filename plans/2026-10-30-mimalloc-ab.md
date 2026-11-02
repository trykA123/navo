# mimalloc A/B (measured, Windows VM)

Decision rule is D11 in `plans/2026-10-29-speed-atoms.md`: mimalloc becomes the default only if it is at least 5 % faster to the warm first usable frame, or uses at least 10 % fewer private bytes, with no listing regression. This plan measures that. It changes no product code.

## Method

- **Two exes, one commit, one machine.** Build A (default allocator) and B (`--features mimalloc`) from the same commit in the Windows VM, both with the `tester` feature and `--release`, each in its own target dir. The CI artifact of B alone is not enough: it is built with `tester`, so A must be too.
- **Script.** `scripts/mimalloc-ab.ps1 -ExeA <a> -ExeB <b> -Work <folder>`. It alternates A and B so drift hits both, throws away one warm-up round, and prints medians.
- **Start-up and memory.** 7 warm `navo --metrics` runs per exe on the 5,000-file and 200 × 25 fixture: first frame, first usable, idle private bytes, peak working set.
- **Big folders.** 3 `navo bench --files 1000000` runs per exe (fresh root each time): 500k-folder list time, index time, index peak private bytes, process peak working set, median Ctrl+K p95.
- **Isolation.** All paths under `C:\navo-test\ab-*`, own profile per exe, no other Navo running, no other VM job. The tester closes each Navo by PID.
- **Noise rule.** If the spread (max − min) of a median is above 5 % of that median, repeat the series once and report both.

## Numbers to send Manwë

One table, rows A and B plus the ratio B/A for: first usable, first frame, idle private MB, peak working set MB, 500k list ms, index ms, index peak private MB, search p95. Then one line: "recommend on / off / unclear", with D11 as the reason.

## Steps

1. Manwë gives the VM slot and a commit that has the gate result.
2. Tester (Arnor) builds A and B in the VM, copies the script over, runs it, returns `ab-summary.json` and the raw JSON.
3. Part lead computes the ratios, writes the table to `docs/perf.md`, sends the recommendation. Nobody turns the feature on.

## Log

- 2026-10-30 — Plan and script written (Tulkas). Waiting for a VM slot.
