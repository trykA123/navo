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

Recording a slow moment (the trace build)

navo-tracy.exe is a separate Navo build that records when its own steps run. The zip also holds
tracy-capture.exe, tracy-csvexport.exe and tracy-profiler.exe, with Tracy's licence in
TRACY-LICENSE.txt.

1. Quit Navo from the tray, so the two copies do not compete for the summon key.
2. Unzip the tester zip into a folder outside Downloads.
3. In that folder, run .\tracy-capture.exe -o navo.tracy -a 127.0.0.1. It waits.
4. Start .\navo-tracy.exe --profile .\tracy-profile. It uses its own settings and never
   touches the real profile.
5. Do the slow thing, then close Navo. tracy-capture writes navo.tracy and stops.
6. Run .\tracy-csvexport.exe navo.tracy > zones.csv and send only zones.csv. It holds fixed
   zone names, empty source columns and times.
7. navo.tracy stays on your PC. Open it with tracy-profiler.exe if you want to look yourself.
   Never commit or publish it.

What a trace holds:
- fixed zone names and their times;
- frame marks;
- thread ids and the names Windows gives threads;
- the exe name, navo-tracy.exe;
- CPU, RAM and OS version;
- the header line "User: navo@tester".

By design it holds no file names, paths, field values, real login, computer name or full name.
The design reasons are:
- zone names are fixed literals in our code;
- source fields are empty;
- the header values are fixed at build time.

What is checked is narrower: before a zip is published, the release workflow scans navo-tracy.exe
and the tester navo.exe for the forbidden words and path fragments, in ASCII and UTF-16LE text.
The publish job fails if it finds one. No capture scan is claimed here.

What leaves your PC is only zones.csv. Its columns are zone name, source file (empty), source
line (0) and times.
