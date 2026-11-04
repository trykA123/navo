navo-bench measures Navo on this PC and writes a report with numbers only.

1. Put navo.exe (the tester build) and navo-bench.exe in the same folder.
2. Run navo-bench.exe. It asks for a folder to measure. The folder is only read.
   Cancel the picker to skip the folder checks; everything else still runs.
3. Wait. The run opens and closes Navo several times. Do not use the PC meanwhile.
4. Open report.html (and report.json) in the output folder. By default this is a new
   folder named navo-bench-<date>-<time> (UTC) next to navo-bench.exe.

The report holds counts, sizes and timings. It never holds file or folder names,
paths, user or machine names, or file contents. Test files are made in a scratch
folder (run-...) next to the output folder and are moved to the Recycle Bin at the end.
The folder you chose is checked before and after the run; if anything in it changed,
the report and the console say "DATA SAFETY: FAIL" and the exit code is 3.

Options:
--folder <folder>  measures this folder and skips the picker.
--quick            skips the generated folder of 100,000 files.
--out <folder>     puts report.html and report.json in this new folder. It must not
                   exist yet, its parent must exist, and the parent must not be a drive
                   root, the Windows or Program Files folder, or your user-profile folder.
                   It must not be inside the folder you measure.

Exit codes: 0 every step ran, 1 a step failed, 2 wrong options or a refused folder,
3 your folder changed or could not be proved unchanged (DATA SAFETY).
