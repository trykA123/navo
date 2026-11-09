<!-- Records actual IX8r Windows VM results and remaining owner checks. -->
# Navo update E2E

Date: 2026-11-09 (Europe/Bucharest). Elevated console driver, session 1.
Case 7 corrected rerun: C:\winvm\ix8r-20261109\run-6bae8a8ba77f457089ada5c9e514bc03

HEAD: 5e2fd1f94db2a4598e4c37591c28deb498d43fe8; revision: 3; package versions: 0.0.1 / 0.0.2 / 0.0.3
Cases 1-6 and 8-9 run: C:\winvm\ix8r-20261109\run-803a934020e1441f9e54efb7d5625436

PASS case1 update: 0.0.2 staged 0.0.3, quit/start applied it, restarted, and cleared staged metadata

    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":null,"state":"idle"}
    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":"0.0.3","state":"ready"}
    EVIDENCE {"kind":"installed","current_version":"0.0.3","staged_version":null,"state":"idle"}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":182,"level":"WARN","target":"navo::search_hooks::index","msg":"cannot connect to the index service pipe","fields":{"error":"NotAvailable"}}
    {"t":5078,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"None"}}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":94,"level":"WARN","target":"navo::search_hooks::index","msg":"cannot connect to the index service pipe","fields":{"error":"NotAvailable"}}

PASS case2 offline: real GitHub feed blocked by executable firewall rule; logged offline; no stage or partial

    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":null,"state":"idle"}
    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":null,"state":"idle"}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":149,"level":"WARN","target":"navo::search_hooks::index","msg":"cannot connect to the index service pipe","fields":{"error":"NotAvailable"}}
    {"t":5052,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"None"}}
    {"t":10060,"level":"WARN","target":"navo::updater::service","msg":"update: check failed","fields":{"error":"offline"}}

PASS case3 bad feed: broken releases.win.json logged bad feed; no stage or partial

    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":null,"state":"idle"}
    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":null,"state":"idle"}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":162,"level":"WARN","target":"navo::search_hooks::index","msg":"cannot connect to the index service pipe","fields":{"error":"NotAvailable"}}
    {"t":5058,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"None"}}
    {"t":10049,"level":"WARN","target":"navo::updater::service","msg":"update: check failed","fields":{"error":"bad feed"}}

PASS case4 interrupted download: killed at 42%; restart applied nothing; a fresh check staged 0.0.3

    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":null,"state":"idle"}
    EVIDENCE interrupted at 41.9% written; partial_bytes=307864422; package_bytes=307864422
    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":null,"state":"idle"}
    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":null,"state":"idle"}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":146,"level":"WARN","target":"navo::search_hooks::index","msg":"cannot connect to the index service pipe","fields":{"error":"NotAvailable"}}
    {"t":5050,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"None"}}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":134,"level":"WARN","target":"navo::search_hooks::index","msg":"cannot connect to the index service pipe","fields":{"error":"NotAvailable"}}
    {"t":5035,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"None"}}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":235,"level":"WARN","target":"navo::search_hooks::index","msg":"cannot connect to the index service pipe","fields":{"error":"NotAvailable"}}
    {"t":5100,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"None"}}

PASS case5 downgrade refused: 0.0.1-only feed left 0.0.2 installed and idle, with nothing staged or applied on restart

    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":null,"state":"idle"}
    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":null,"state":"idle"}
    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":null,"state":"idle"}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":167,"level":"WARN","target":"navo::search_hooks::index","msg":"cannot connect to the index service pipe","fields":{"error":"NotAvailable"}}
    {"t":5049,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"None"}}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":147,"level":"WARN","target":"navo::search_hooks::index","msg":"cannot connect to the index service pipe","fields":{"error":"NotAvailable"}}

PASS case6 portable no request: portable metadata unsupported; explicit no-automatic-checks log; nothing staged

    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":36,"level":"INFO","target":"navo::updater::service","msg":"update: no automatic checks for this copy","fields":{"kind":"Portable"}}
    {"t":176,"level":"WARN","target":"navo::search_hooks::index","msg":"cannot connect to the index service pipe","fields":{"error":"NotAvailable"}}
    {"t":5050,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"None"}}

PASS case7 second process blocks apply: second profile kept its window and blocked apply; after both quit, next start applied N+1

    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":null,"state":"idle"}
    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":"0.0.3","state":"ready"}
    EVIDENCE second_pid=2584; second_window=1114300; profiles=2; pdf_helpers=2; current=0.0.2; staged=0.0.3
    EVIDENCE {"kind":"installed","current_version":"0.0.3","staged_version":null,"state":"idle"}
    {"t":409,"level":"WARN","target":"navo::search_hooks::index","msg":"cannot connect to the index service pipe","fields":{"error":"NotAvailable"}}
    {"t":5068,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"None"}}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":218,"level":"WARN","target":"navo::search_hooks::index","msg":"cannot connect to the index service pipe","fields":{"error":"NotAvailable"}}
    {"t":5121,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"None"}}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":146,"level":"WARN","target":"navo::search_hooks::index","msg":"cannot connect to the index service pipe","fields":{"error":"NotAvailable"}}
    {"t":5050,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"None"}}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":126,"level":"WARN","target":"navo::search_hooks::index","msg":"cannot connect to the index service pipe","fields":{"error":"NotAvailable"}}

OWNER-CHECK case8 service revision bump: revision, image, repeat start and clean uninstall verified; exactly one UAC prompt needs an interactive owner check

    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":null,"state":"idle"}
    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":"0.0.3","state":"ready"}
    EVIDENCE {"kind":"installed","current_version":"0.0.3","staged_version":null,"state":"idle"}
    EVIDENCE revision=3; pipe_pid=2720; scm_pid=2720; image=C:\Program Files\Navo\Index\navo-index.exe; description=Navo Index
    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":null,"state":"idle"}
    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":"0.0.3","state":"ready"}
    EVIDENCE {"kind":"installed","current_version":"0.0.3","staged_version":null,"state":"idle"}
    EVIDENCE revision=4; pipe_pid=4088; scm_pid=4088; image=C:\Program Files\Navo\Index\navo-index.exe; description=Navo Index
    EVIDENCE legacy_navo.exe=False; service_after_uninstall=False; Index_after_uninstall=False
    {"t":5024,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"Some(Known { revision: 3, build: \"\u003ctext:6 chars\u003e\" })"}}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":5056,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"Some(Known { revision: 3, build: \"\u003ctext:6 chars\u003e\" })"}}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":5031,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Elevate","version":"Some(Known { revision: 3, build: \"\u003ctext:6 chars\u003e\" })"}}
    {"t":5033,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Elevate","version":"Some(Known { revision: 3, build: \"\u003ctext:6 chars\u003e\" })"}}
    {"t":5033,"level":"INFO","target":"navo::service_version_step::worker","msg":"index service is older than Navo; asking to update it","fields":{"own":4}}
    {"t":12375,"level":"INFO","target":"navo::service_version_step::worker","msg":"index service updated","fields":{"version":"Some(Known { revision: 4, build: \"\u003ctext:6 chars\u003e\" })"}}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":5057,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"Some(Known { revision: 4, build: \"\u003ctext:6 chars\u003e\" })"}}

OWNER-CHECK case9 legacy service migration and remove: revision, image, repeat start and clean uninstall verified; exactly one UAC prompt needs an interactive owner check

    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":null,"state":"idle"}
    EVIDENCE {"kind":"installed","current_version":"0.0.2","staged_version":"0.0.3","state":"ready"}
    EVIDENCE {"kind":"installed","current_version":"0.0.3","staged_version":null,"state":"idle"}
    EVIDENCE revision=4; pipe_pid=3364; scm_pid=3364; image=C:\Program Files\Navo\Index\navo-index.exe; description=Navo Index
    EVIDENCE legacy_navo.exe=False; service_after_uninstall=False; Index_after_uninstall=False
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":5053,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"Some(Known { revision: 3, build: \"\u003ctext:6 chars\u003e\" })"}}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":5024,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Elevate","version":"Some(Known { revision: 3, build: \"\u003ctext:6 chars\u003e\" })"}}
    {"t":5025,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Elevate","version":"Some(Known { revision: 3, build: \"\u003ctext:6 chars\u003e\" })"}}
    {"t":5025,"level":"INFO","target":"navo::service_version_step::worker","msg":"index service is older than Navo; asking to update it","fields":{"own":4}}
    {"t":10840,"level":"INFO","target":"navo::service_version_step::worker","msg":"index service updated","fields":{"version":"Some(Known { revision: 4, build: \"\u003ctext:6 chars\u003e\" })"}}
    {"t":0,"level":"INFO","target":"navo::logging","msg":"app started","fields":{"version":"0.3.0","commit":"5e2fd1f94db2"}}
    {"t":5080,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"Some(Known { revision: 4, build: \"\u003ctext:6 chars\u003e\" })"}}


Final script SHA256: 6CAFF4DFFD5F0629C2DBA3D45AC92CD282E7A34498076194BFF67722B80C89DF

Verification and reproduction:
- Reached the VM only through winvm.sh ssh/scp. Used a git bundle and a detached checkout at C:\src\navo-e2e; C:\src\navo was not touched.
- Every build and E2E invocation held the host VM lock. No Linux Cargo builds, VM restart, server, commit, push or deployment.
- Windows PowerShell 5.1 Parser.ParseFile: PASS, including the final script before the case-7 rerun.
- cargo build --locked --release -p navo-app --features tester and cargo build --locked --release -p navo-index-service: PASS for previous (0.0.1), base (0.0.2), next (0.0.3), and bumped (0.0.3, service revision 4). Used CARGO_PROFILE_RELEASE_LTO=thin for the 8 GB VM, retained PDBs, and packed with vpk 1.2.161. Runtime and packaging tools lived in this run's isolated directory.
- Console task command: scripts\update-e2e.ps1 -Out C:\winvm\ix8r-20261109\report.md -WorkDir C:\winvm\ix8r-20261109 -SkipBuild -ElevatedSession. Cases 1-6 passed; cases 8-9 verified the requested service behavior. Case 7 initially failed a harness count that included PDF helpers.
- Corrected rerun: the same command with -Cases @(7) and -Out C:\winvm\ix8r-20261109\case7-report.md. PASS; exit 0. Its complete case block replaces the earlier failed count above. Original reports and transcripts remain in the VM work directory.
- cargo xtask check: not run; no Rust crate was touched. No product code or release feed rule changed.

Script bugs fixed:
- vpk 1.2.161 rejects --version; read its --help version banner.
- SSH session 0 cannot inspect or close session-1 windows, and service installation there reports no interactive user. Run the driver as an elevated interactive task; the script now rejects session 0.
- Debug targets are navo::, not navo_app::. The wrong filter hid service-revision observations.
- Windows preallocates the partial file at full length. Case 4 now pads only its temporary feed package, updates its checksums, measures real process writes, and kills the downloader directly. It observed 41.9% written with a full-length partial and verified a clean retry.
- Case 7's GUI count included --pdf-helper children. Count profiles separately, retain the second-window check, and stop subsequent cases if that window is closed or killed (D22d).
- Elevated service cases report OWNER-CHECK for prompt counting after all observable assertions, including clean uninstall in both cases. Reports preserve checked metadata and service evidence; script exit codes are explicit.

Owner checks and limitations:
- Cases 8-9 still require exactly one accepted UAC prompt on a normal interactive owner session. Elevated execution cannot prove that count. Both cases verified revision 4, matching pipe/SCM PID, navo-index.exe with description Navo Index, no legacy navo.exe, no repeat update request, and no Index folder after uninstall.
- An existing VM CI run for 5e2fd1f9 reported 12,238 passed, one failed, and 62 skipped: updater::feed::tests::release_feed_never_reads_the_test_override at feed_tests.rs:75. It assumes LF source text and fails with the Windows checkout's CRLF. This pre-existing test failure was not changed in this script-only task.
- Cleanup checked after the final rerun: no NavoApp install, Navo Index service, Index folder, navo/navo-index process, E2E firewall rule, or E2E scheduled task remains. No server was started.
