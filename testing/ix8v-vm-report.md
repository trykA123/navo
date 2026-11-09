<!-- Records IX8v Windows service VM observations and human follow-up. -->
# IX8v index-service VM report

HEAD: 5a8b49dee87a6865fd0d981ef5ceafff799abd4b; session=1; elevated driver; UAC cannot be counted.

Fixture navo.exe: SHA256=E2EC5138ED6E2D7DEBD409D644FEF0A5EAD97607734FAAC77C63D3A35AF6C036

Fixture NavoApp-win-Setup.exe: SHA256=708358FBF67749E75AC6CFB2AAB68869C705D0FC8A9C3D7CF4AC8E56978DDC26

Fixture NavoApp-win-Setup.exe: SHA256=E4501CF3E5961109B04EB3B732EE67E0277DDBA6D466E6A2B99D958AFF6EC67A

Fixture revision2.exe: SHA256=93BD0C58A9DD277675B5255EB07F758CC4BB26C038C3B76C3DD6A12DB61F14AA

Current service SHA256=E844BD16FA44BEB126EA8E3FC437FE5CE1F480C9D2EF1AD18207B2DFF4F9A100; v0.2.2 Setup followed by IX8v Setup.


OWNER-CHECK check1: normal unelevated session: install v0.2.2 with Setup, install IX8v with Setup, accept exactly one UAC prompt; verify Task Manager Navo Index and Settings Running; restart and verify no prompt.

    EVIDENCE remove exit=0; seconds=0.65;  navo: index service: kept unknown C:\Program Files\Navo\change.lock
    EVIDENCE command=navo.exe --index-service install; exit=0; seconds=2.55;
    EVIDENCE command=sc.exe description Navo Index IX8v legacy rollback description; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failure Navo Index reset= 86400 actions= restart/7000/restart/11000/none/0; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failureflag Navo Index 1; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE state=Running; command="C:\Program Files\Navo\Index\navo-index.exe" run; description=Keeps the file index for Navo search up to date.; pipe_pid=5308; scm_pid=5308; files=navo-index.exe
    EVIDENCE {"t":5104,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Elevate","version":"Some(Known { revision: 2, build: \"\u003ctext:6 chars\u003e\" })"}}
    EVIDENCE {"t":5106,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Elevate","version":"Some(Known { revision: 2, build: \"\u003ctext:6 chars\u003e\" })"}}
    EVIDENCE {"t":5106,"level":"INFO","target":"navo::service_version_step::worker","msg":"index service is older than Navo; asking to update it","fields":{"own":3}}
    EVIDENCE {"t":12102,"level":"INFO","target":"navo::service_version_step::worker","msg":"index service updated","fields":{"version":"Some(Known { revision: 3, build: \"\u003ctext:6 chars\u003e\" })"}}
    EVIDENCE {"t":5034,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"Some(Known { revision: 3, build: \"\u003ctext:6 chars\u003e\" })"}}

OWNER-CHECK check2: normal unelevated session with revision-2 service: decline the first prompt, restart the same profile, verify no second prompt; Settings > Search must show Older than Navo and Update; click Update, accept exactly one prompt, verify Running.

    EVIDENCE remove exit=0; seconds=0.54;  navo: index service: kept unknown C:\Program Files\Navo\change.lock
    EVIDENCE command=navo.exe --index-service install; exit=0; seconds=1.58;
    EVIDENCE command=sc.exe description Navo Index IX8v legacy rollback description; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failure Navo Index reset= 86400 actions= restart/7000/restart/11000/none/0; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failureflag Navo Index 1; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE automatic failure marker=3; this tests the shared failed/declined path, not a UAC cancellation
    EVIDENCE install exit=0; seconds=9.99;
    EVIDENCE state=Running; command="C:\Program Files\Navo\Index\navo-index.exe" run; description=Keeps the file index for Navo search up to date.; pipe_pid=7816; scm_pid=7816; files=navo-index.exe
    EVIDENCE {"t":5147,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Elevate","version":"Some(Known { revision: 2, build: \"\u003ctext:6 chars\u003e\" })"}}
    EVIDENCE {"t":5148,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Elevate","version":"Some(Known { revision: 2, build: \"\u003ctext:6 chars\u003e\" })"}}
    EVIDENCE {"t":5148,"level":"INFO","target":"navo::service_version_step::worker","msg":"index service is older than Navo; asking to update it","fields":{"own":3}}
    EVIDENCE {"t":12397,"level":"WARN","target":"navo::service_version_step::worker","msg":"index service update declined or failed; not asking again for this revision","fields":{"reason":"Some(\"\u003ctext:87 chars\u003e\")","version":"Some(Known { revision: 2, build: \"\u003ctext:6 chars\u003e\" })"}}
    EVIDENCE {"t":5031,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Outdated","version":"Some(Known { revision: 2, build: \"\u003ctext:6 chars\u003e\" })"}}

OWNER-CHECK check2b: normal unelevated session with revision-2 service: launch two installed profiles simultaneously; count exactly one UAC prompt in total; accept it and verify both profiles show the migrated service.

    EVIDENCE remove exit=0; seconds=1.17;  navo: index service: kept unknown C:\Program Files\Navo\change.lock
    EVIDENCE command=navo.exe --index-service install; exit=0; seconds=1.53;
    EVIDENCE command=sc.exe description Navo Index IX8v legacy rollback description; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failure Navo Index reset= 86400 actions= restart/7000/restart/11000/none/0; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failureflag Navo Index 1; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE state=Running; command="C:\Program Files\Navo\Index\navo-index.exe" run; description=Keeps the file index for Navo search up to date.; pipe_pid=1360; scm_pid=1360; files=navo-index.exe
    EVIDENCE {"t":5423,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Elevate","version":"Some(Known { revision: 2, build: \"\u003ctext:6 chars\u003e\" })"}}
    EVIDENCE {"t":5424,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Elevate","version":"Some(Known { revision: 2, build: \"\u003ctext:6 chars\u003e\" })"}}
    EVIDENCE {"t":5424,"level":"INFO","target":"navo::service_version_step::worker","msg":"index service is older than Navo; asking to update it","fields":{"own":3}}
    EVIDENCE {"t":19402,"level":"INFO","target":"navo::service_version_step::worker","msg":"index service updated","fields":{"version":"Some(Known { revision: 3, build: \"\u003ctext:6 chars\u003e\" })"}}
    EVIDENCE {"t":5582,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"None"}}

OWNER-CHECK check3: stop the revision-2 navo.exe service, start the installed IX8v app unelevated, verify no prompt; Settings > Search must offer Update; click it, accept one prompt and verify Running.

    EVIDENCE remove exit=0; seconds=0.64;  navo: index service: kept unknown C:\Program Files\Navo\change.lock
    EVIDENCE command=navo.exe --index-service install; exit=0; seconds=1.47;
    EVIDENCE command=sc.exe description Navo Index IX8v legacy rollback description; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failure Navo Index reset= 86400 actions= restart/7000/restart/11000/none/0; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failureflag Navo Index 1; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE old_pid=6388; state=Stopped; command="C:\Program Files\Navo\Index\navo.exe" --index-service run
    EVIDENCE install exit=0; seconds=0.52;
    EVIDENCE state=Running; command="C:\Program Files\Navo\Index\navo-index.exe" run; description=Keeps the file index for Navo search up to date.; pipe_pid=8640; scm_pid=8640; files=navo-index.exe
    EVIDENCE {"t":188,"level":"WARN","target":"navo::search_hooks::index","msg":"cannot connect to the index service pipe","fields":{"error":"NotAvailable"}}
    EVIDENCE {"t":5074,"level":"DEBUG","target":"navo::service_version_step::worker","msg":"index service step","fields":{"step":"Nothing","version":"None"}}

PASS check4a: refused run restored legacy command, description, recovery, delayed start, hash and Running; new image absent

    EVIDENCE remove exit=0; seconds=0.54;  navo: index service: kept unknown C:\Program Files\Navo\change.lock
    EVIDENCE command=navo.exe --index-service install; exit=0; seconds=1.4;
    EVIDENCE command=sc.exe description Navo Index IX8v legacy rollback description; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failure Navo Index reset= 86400 actions= restart/7000/restart/11000/none/0; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failureflag Navo Index 1; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE before={"Path":"\"C:\\Program Files\\Navo\\Index\\navo.exe\" --index-service run","Description":"IX8v legacy rollback description","Recovery":"[SC] QueryServiceConfig2 SUCCESS\r\n\r\nSERVICE_NAME: Navo Index\r\n        RESET_PERIOD (in seconds)    : 86400\r\n        REBOOT_MESSAGE               : \r\n        COMMAND_LINE                 : \r\n        FAILURE_ACTIONS              : RESTART -- Delay = 7000 milliseconds.\r\n                                       RESTART -- Delay = 11000 milliseconds.\r\n\r\n","FailureFlag":"[SC] QueryServiceConfig2 SUCCESS\r\n\r\nSERVICE_NAME: Navo Index\r\nFAILURE_ACTIONS_ON_NONCRASH_FAILURES:  TRUE\r\n","Delayed":1}; image_sha256=E2EC5138ED6E2D7DEBD409D644FEF0A5EAD97607734FAAC77C63D3A35AF6C036
    EVIDENCE install exit=4; seconds=2.38;  navo: index service: The service did not respond to the start or control request in a timely fashion. (os error 1053)
    EVIDENCE after={"Path":"\"C:\\Program Files\\Navo\\Index\\navo.exe\" --index-service run","Description":"IX8v legacy rollback description","Recovery":"[SC] QueryServiceConfig2 SUCCESS\r\n\r\nSERVICE_NAME: Navo Index\r\n        RESET_PERIOD (in seconds)    : 86400\r\n        REBOOT_MESSAGE               : \r\n        COMMAND_LINE                 : \r\n        FAILURE_ACTIONS              : RESTART -- Delay = 7000 milliseconds.\r\n                                       RESTART -- Delay = 11000 milliseconds.\r\n\r\n","FailureFlag":"[SC] QueryServiceConfig2 SUCCESS\r\n\r\nSERVICE_NAME: Navo Index\r\nFAILURE_ACTIONS_ON_NONCRASH_FAILURES:  TRUE\r\n","Delayed":1}; state=Running; files=navo.exe

PASS check4b: revision-2 answer returned exit 3 and restored complete legacy state

    EVIDENCE remove exit=0; seconds=0.65;  navo: index service: kept unknown C:\Program Files\Navo\change.lock
    EVIDENCE command=navo.exe --index-service install; exit=0; seconds=1.37;
    EVIDENCE command=sc.exe description Navo Index IX8v legacy rollback description; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failure Navo Index reset= 86400 actions= restart/7000/restart/11000/none/0; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failureflag Navo Index 1; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE before={"Path":"\"C:\\Program Files\\Navo\\Index\\navo.exe\" --index-service run","Description":"IX8v legacy rollback description","Recovery":"[SC] QueryServiceConfig2 SUCCESS\r\n\r\nSERVICE_NAME: Navo Index\r\n        RESET_PERIOD (in seconds)    : 86400\r\n        REBOOT_MESSAGE               : \r\n        COMMAND_LINE                 : \r\n        FAILURE_ACTIONS              : RESTART -- Delay = 7000 milliseconds.\r\n                                       RESTART -- Delay = 11000 milliseconds.\r\n\r\n","FailureFlag":"[SC] QueryServiceConfig2 SUCCESS\r\n\r\nSERVICE_NAME: Navo Index\r\nFAILURE_ACTIONS_ON_NONCRASH_FAILURES:  TRUE\r\n","Delayed":1}; image_sha256=E2EC5138ED6E2D7DEBD409D644FEF0A5EAD97607734FAAC77C63D3A35AF6C036
    EVIDENCE install exit=3; seconds=32.79;  navo: index service: the service answered Known { revision: 2, build: "dev" }, not revision 3
    EVIDENCE after={"Path":"\"C:\\Program Files\\Navo\\Index\\navo.exe\" --index-service run","Description":"IX8v legacy rollback description","Recovery":"[SC] QueryServiceConfig2 SUCCESS\r\n\r\nSERVICE_NAME: Navo Index\r\n        RESET_PERIOD (in seconds)    : 86400\r\n        REBOOT_MESSAGE               : \r\n        COMMAND_LINE                 : \r\n        FAILURE_ACTIONS              : RESTART -- Delay = 7000 milliseconds.\r\n                                       RESTART -- Delay = 11000 milliseconds.\r\n\r\n","FailureFlag":"[SC] QueryServiceConfig2 SUCCESS\r\n\r\nSERVICE_NAME: Navo Index\r\nFAILURE_ACTIONS_ON_NONCRASH_FAILURES:  TRUE\r\n","Delayed":1}; state=Running; files=navo.exe

PASS check4c: failed reinstall restored previous navo-index.exe hash, complete SCM configuration and Running

    EVIDENCE remove exit=0; seconds=0.59;  navo: index service: kept unknown C:\Program Files\Navo\change.lock
    EVIDENCE command=navo.exe --index-service install; exit=0; seconds=1.5;
    EVIDENCE command=sc.exe description Navo Index IX8v legacy rollback description; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failure Navo Index reset= 86400 actions= restart/7000/restart/11000/none/0; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failureflag Navo Index 1; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE install exit=0; seconds=1.05;
    EVIDENCE before={"Path":"\"C:\\Program Files\\Navo\\Index\\navo-index.exe\" run","Description":"Keeps the file index for Navo search up to date.","Recovery":"[SC] QueryServiceConfig2 SUCCESS\r\n\r\nSERVICE_NAME: Navo Index\r\n        RESET_PERIOD (in seconds)    : 86400\r\n        REBOOT_MESSAGE               : \r\n        COMMAND_LINE                 : \r\n        FAILURE_ACTIONS              : RESTART -- Delay = 5000 milliseconds.\r\n                                       RESTART -- Delay = 60000 milliseconds.\r\n\r\n","FailureFlag":"[SC] QueryServiceConfig2 SUCCESS\r\n\r\nSERVICE_NAME: Navo Index\r\nFAILURE_ACTIONS_ON_NONCRASH_FAILURES:  FALSE\r\n","Delayed":1}; image_sha256=E844BD16FA44BEB126EA8E3FC437FE5CE1F480C9D2EF1AD18207B2DFF4F9A100
    EVIDENCE install exit=4; seconds=2.85;  navo: index service: The service did not respond to the start or control request in a timely fashion. (os error 1053)
    EVIDENCE after={"Path":"\"C:\\Program Files\\Navo\\Index\\navo-index.exe\" run","Description":"Keeps the file index for Navo search up to date.","Recovery":"[SC] QueryServiceConfig2 SUCCESS\r\n\r\nSERVICE_NAME: Navo Index\r\n        RESET_PERIOD (in seconds)    : 86400\r\n        REBOOT_MESSAGE               : \r\n        COMMAND_LINE                 : \r\n        FAILURE_ACTIONS              : RESTART -- Delay = 5000 milliseconds.\r\n                                       RESTART -- Delay = 60000 milliseconds.\r\n\r\n","FailureFlag":"[SC] QueryServiceConfig2 SUCCESS\r\n\r\nSERVICE_NAME: Navo Index\r\nFAILURE_ACTIONS_ON_NONCRASH_FAILURES:  FALSE\r\n","Delayed":1}; state=Running; files=navo-index.exe
    EVIDENCE state=Running; command="C:\Program Files\Navo\Index\navo-index.exe" run; description=Keeps the file index for Navo search up to date.; pipe_pid=3152; scm_pid=3152; files=navo-index.exe

FAIL check5: remove left Program Files\Navo; no workaround applied

    EVIDENCE remove exit=0; seconds=0.65;  navo: index service: kept unknown C:\Program Files\Navo\change.lock
    EVIDENCE command=navo.exe --index-service install; exit=0; seconds=1.55;
    EVIDENCE command=sc.exe description Navo Index IX8v legacy rollback description; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failure Navo Index reset= 86400 actions= restart/7000/restart/11000/none/0; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failureflag Navo Index 1; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE install exit=0; seconds=1.04;
    EVIDENCE remove exit=0; seconds=0.54;  navo: index service: kept unknown C:\Program Files\Navo\change.lock
    EVIDENCE service_after_remove=False; Index_after_remove=False; Navo_after_remove=True
    EVIDENCE command=Update.exe uninstall --silent; exit=0; seconds=3.39; [12:11:43] [INFO] -- | [12:11:43] [INFO] Starting Velopack Updater (1.2.161) | [12:11:43] [INFO]     Location: "C:\\Users\\admin\\AppData\\Local\\NavoApp\\Update.exe" | [12:11:43] [INFO]     CWD: "C:\\Users\\admin\\AppData\\Local\\NavoApp" | [12:11:43] [INFO]     Verbose: false | [12:11:43] [INFO]     Silent: true | [12:11:43] [INFO]     Root Directory: None | [12:11:43] [INFO]     Log File: "C:\\Users\\admin\\AppData\\Local\\velopack\\velopack_NavoApp.log" | [12:11:43] [INFO]     Context: IAmUpdateExe | [12:11:43] [INFO] Command: Uninstall | [12:11:43] [INFO] Auto-locating app manifest... | [12:11:43] [INFO] Root directory 'C:\Users\admin\AppData\Local\NavoApp' writable: true | [12:11:43] [INFO] Using root packages directory: C:\Users\admin\AppData\Local\NavoApp\packages | [12:11:43] [INFO] Command: Uninstall | [12:11:43] [INFO] Checking for running processes in: C:\Users\admin\AppData\Local\NavoApp | [12:11:43] [INFO] Inspected 118 running processes in 11ms, 1 matched directory | [12:11:43] [WARN] Skipping killing self: C:\Users\admin\AppData\Local\NavoApp\Update.exe (9168) | [12:11:43] [INFO] Force-stop check complete. | [12:11:43] [INFO] Running --veloapp-uninstall hook... | [12:11:43] [INFO] About to launch: 'WideString("C:\\Users\\admin\\AppData\\Local\\NavoApp\\current\\navo.exe")' in dir 'Some(WideString("C:\\Users\\admin\\AppData\\Local\\NavoApp\\current"))' with arguments: WideString("\"C:\\Users\\admin\\AppData\\Local\\NavoApp\\current\\navo.exe\" --veloapp-uninstall 0.3.1\0") | [12:11:43] [INFO] Environment block present: true, flags: PROCESS_CREATION_FLAGS(134218752) | [12:11:43] [INFO] Waiting 60000ms for process handle to exit. | [12:11:43] [INFO] Hook executed successfully (took 35.1771ms) | [12:11:43] [INFO] Checking for running processes in: C:\Users\admin\AppData\Local\NavoApp | [12:11:43] [INFO] Inspected 118 running processes in 10ms, 1 matched directory | [12:11:43] [WARN] Skipping killing self: C:\Users\admin\AppData\Local\NavoApp\Update.exe (9168) | [12:11:43] [INFO] Force-stop check complete. | [12:11:43] [INFO] COM context initialized in 5ms | [12:11:43] [INFO] Searching for shortcuts containing root: '"C:\\Users\\admin\\AppData\\Local\\NavoApp"' | [12:11:43] [INFO] Searching for shortcuts in: ShortcutLocationFlags(DESKTOP) (C:\Users\admin\Desktop/*.lnk) | [12:11:43] [INFO] Selected shortcut for update '"C:\\Users\\admin\\Desktop\\Navo.lnk"' because target '"C:\\Users\\admin\\AppData\\Local\\NavoApp\\current\\navo.exe"' matched. | [12:11:43] [INFO] Searching for shortcuts in: ShortcutLocationFlags(STARTUP) (C:\Users\admin\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup/*.lnk) | [12:11:43] [INFO] Searching for shortcuts in: ShortcutLocationFlags(START_MENU) (C:\Users\admin\AppData\Roaming\Microsoft\Windows\Start Menu\Programs/**/*.lnk) | [12:11:43] [INFO] Selected shortcut for update '"C:\\Users\\admin\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs\\Navo.lnk"' because target '"C:\\Users\\admin\\AppData\\Local\\NavoApp\\current\\navo.exe"' matched. | [12:11:43] [INFO] Searching for shortcuts in: ShortcutLocationFlags(USER_PINNED) (C:\Users\admin\AppData\Roaming\Microsoft\Internet Explorer\Quick Launch\User Pinned/**/*.lnk) | [12:11:43] [INFO] Removing shortcut '"C:\\Users\\admin\\Desktop\\Navo.lnk"' (ShortcutLocationFlags(DESKTOP)). | [12:11:43] [INFO] Removing shortcut '"C:\\Users\\admin\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs\\Navo.lnk"' (ShortcutLocationFlags(START_MENU)). | [12:11:43] [INFO] Removing directory '"C:\\Users\\admin\\AppData\\Local\\NavoApp"' | [12:11:43] [INFO] Removing directory '"C:\\Users\\admin\\AppData\\Local\\Temp\\velopack_NavoApp"' | [12:11:43] [INFO] Removing uninstall registry keys... | [12:11:43] [INFO] Finished successfully. | [12:11:43] [INFO] Scheduling removal of install directory... | [12:11:43] [INFO] Running: cmd.exe /C choice /C Y /N /D Y /T 3 & rmdir /s /q "C:\Users\admin\AppData\Local\NavoApp" | Y

PASS check7: unelevated global mutex ignored; exclusive file contention timed out at 60/20 seconds with exit 4; released lock acquired

    EVIDENCE remove exit=0; seconds=0.66;  navo: index service: kept unknown C:\Program Files\Navo\change.lock
    EVIDENCE command=navo.exe --index-service install; exit=0; seconds=2.97;
    EVIDENCE command=sc.exe description Navo Index IX8v legacy rollback description; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failure Navo Index reset= 86400 actions= restart/7000/restart/11000/none/0; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failureflag Navo Index 1; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE holder_task=NavoIndexE2E-2b3929f36aac49db9b916acff4b0a892; execute=C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe; arguments=-NoProfile -ExecutionPolicy Bypass -File "C:\winvm\ix8v-20261109\contention-final\check7-dd082347f81443babd3fc71fc634aa10\hold-73df5d50dcc34da3abc816def1991af9.ps1" -Config "C:\winvm\ix8v-20261109\contention-final\check7-dd082347f81443babd3fc71fc634aa10\hold-73df5d50dcc34da3abc816def1991af9.json"
    EVIDENCE pid=2108; mode=Global; elevated=False
    EVIDENCE install exit=0; seconds=1.36;
    EVIDENCE state=Running; command="C:\Program Files\Navo\Index\navo-index.exe" run; description=Keeps the file index for Navo search up to date.; pipe_pid=2260; scm_pid=2260; files=navo-index.exe
    EVIDENCE pid=5572; mode=Lock; elevated=True
    EVIDENCE install exit=4; seconds=60.06;  navo: index service: another index service change held change.lock until the wait ended
    EVIDENCE remove exit=4; seconds=20.02;  navo: index service: another index service change held change.lock until the wait ended
    EVIDENCE install exit=0; seconds=0.03;

FAIL check6: reboot cleanup or refused-image exit differs from the VM plan; see evidence; Settings deferred notice still OWNER-CHECK

    EVIDENCE remove exit=0; seconds=0.65;  navo: index service: kept unknown C:\Program Files\Navo\change.lock
    EVIDENCE command=navo.exe --index-service install; exit=0; seconds=1.35;
    EVIDENCE command=sc.exe description Navo Index IX8v legacy rollback description; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failure Navo Index reset= 86400 actions= restart/7000/restart/11000/none/0; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE command=sc.exe failureflag Navo Index 1; exit=0; seconds=0.01; [SC] ChangeServiceConfig2 SUCCESS
    EVIDENCE install exit=0; seconds=1.06;
    EVIDENCE pid=7288; mode=Refused; elevated=True
    EVIDENCE remove exit=7; seconds=1.35;  navo: index service: cannot delete or rename service image C:\Program Files\Navo\Index\navo-index.exe: The process cannot access the file because it is being used by another process. (os error 32)
    EVIDENCE refused_delete_and_rename_exit=7; image_exists=True; pending=
    EVIDENCE remove exit=0; seconds=0.02;  navo: index service: kept unknown C:\Program Files\Navo\change.lock
    EVIDENCE install exit=0; seconds=0.28;
    EVIDENCE pid=1240; mode=Mapped; elevated=True
    EVIDENCE remove exit=6; seconds=1.45;  navo: index service: C:\Program Files\Navo\Index\navo-index.exe.8224.old is deleted at the next restart | navo: index service: C:\Program Files\Navo\Index is deleted at the next restart | navo: index service: kept unknown C:\Program Files\Navo\change.lock
    EVIDENCE deferred_exit=6; service_left=False; leftovers=C:\Program Files\Navo\Index;C:\Program Files\Navo\change.lock;C:\Program Files\Navo\Index\navo-index.exe.8224.old
    EVIDENCE pending=*1\??\C:\Program Files\Navo\Index\navo-index.exe.8224.old;;*1\??\C:\Program Files\Navo\Index;

Automated cleanup before reboot: service=False; app_install=False; Navo_folder=True. Leftovers stay intact for reboot observation.

    EVIDENCE boot_before=2026-11-09T08:51:24.5000000Z; boot_after=2026-11-09T10:15:35.5000000Z; service_left=False; Index_left=False; Navo_left=True; refused_exit=7
    EVIDENCE leftovers=C:\Program Files\Navo\change.lock


Date: 2026-11-09 (Europe/Bucharest). VM: Windows 11, Windows PowerShell 5.1, elevated console session 1.

Run details and limits:

- The baseline is the published v0.2.2 Setup and standalone `navo.exe`. Both SHA256 values matched the release's `SHA256SUMS.txt`.
- The current runtime binaries reuse IX8r's `next/dist`, built at `5e2fd1f9`, repacked as version 0.3.1 with vpk 1.2.161. The runtime Rust sources match this checkout; the intervening Rust diff changes only an updater source-text test.
- The revision-2 replacement is the real IX8u service built from `73a30584`, not a changed constant in the current service. Windows `cargo build --locked --release -p navo-index-service` printed `Finished release profile [optimized + debuginfo] ... in 1m 00s`. The post-build PowerShell `Start-Process -Wait` wrapper hung after Cargo exited; the wrapper was stopped and its completed executable preserved.
- Only `C:\src\navo-e2e` was reset. No Linux Cargo build, product source edit, commit, push, deployment or server was part of this task.
- A UAC cancellation cannot be produced by this elevated driver. Check 2 exercised a genuine failed automatic install, the shared failure/decline marker, and its repeat-start suppression. Actual cancellation and Settings controls remain owner checks.
- Check 5 exercised CLI remove followed by silent uninstall. Clicking Settings Remove and the uninstall callback's Removed log within 25 seconds were not independently verified here; IX8r's report covers its earlier uninstall observations.

Findings:

1. Confirmed cleanup bug: `crates/navo-index-service/src/change_lock/file.rs:38` creates the Navo parent and lock; `crates/navo-index-service/src/removal/files.rs:94` treats that lock as an unknown parent entry and returns before removing or scheduling the parent. Ordinary remove returned 0 with `kept unknown C:\Program Files\Navo\change.lock`. Deferred remove returned 6; after one real reboot the renamed image and Index folder were gone, while the lock and Navo parent remained. Checks 5 and 6 fail the requested full-folder assertion. No workaround changed either observation.
2. Plan/code drift: `crates/navo-index-service/src/elevation_result.rs:25` maps a locked `navo-index.exe` to exit 7. The VM returned 7 when both delete and rename were denied, with no image scheduled. The VM plan at `plans/2026-10-27-beta.md:489` says exit 1. This is an intentional dedicated exit code in this branch, reported as a mismatch rather than silently accepted.

Owner steps:

1. Use a normal, unelevated interactive session. Install v0.2.2 through Setup, install its revision-2 service, then install the IX8v build through Setup. Check 1: accept exactly one UAC prompt; inspect Task Manager's Navo Index description, Services' description and Settings > Search Running; restart and verify no prompt.
2. Recreate the old running service. Check 2: decline the first prompt; restart the same profile; verify no repeat prompt and Older than Navo with Update. Click Update, accept one prompt and verify Running.
3. Recreate the old running service. Check 2b: launch two distinct installed profiles together; accept exactly one prompt total and verify migration.
4. Recreate the old service and stop it. Check 3: launch IX8v, verify no prompt; Settings > Search must offer Update; accept its prompt and verify Running.
5. Check 5: use Settings Remove, then uninstall; inspect the uninstall log and the 25-second callback requirement. Check 6: map the installed service image with this script's Mapped holder, click Remove and verify the exact deferred notice: "The index service is removed. Windows deletes its last files at the next restart." The reboot portion was executed here; only the Settings notice remains manual.

Reproduction and verification:

The host launcher uses the existing `winvm.sh ssh|scp` commands. Each VM build, test run and restart holds `flock "$VM_LOCK"`; use `flock --close` when launching the daemonized VM so QEMU does not inherit the lock descriptor. The VM was stopped and started once. No second reboot occurred.

Run from `C:\src\navo-e2e` in an elevated interactive scheduled task:

```powershell
.\scripts\index-service-e2e.ps1 `
  -Out C:\winvm\ix8v-20261109\report.md `
  -WorkDir C:\winvm\ix8v-20261109 `
  -LegacyExe C:\winvm\navo.exe `
  -LegacySetup C:\winvm\NavoApp-win-Setup.exe `
  -Setup C:\winvm\ix8v-20261109\out\NavoApp-win-Setup.exe `
  -Revision2Exe C:\winvm\ix8v-20261109\revision2.exe
```

After the permitted stop/start and SSH readiness, the reboot phase can run through elevated SSH:

```powershell
.\scripts\index-service-e2e.ps1 -Phase AfterReboot `
  -Out C:\winvm\ix8v-20261109\report.md `
  -WorkDir C:\winvm\ix8v-20261109
```

The VM retains the complete transcripts, initial reports, scheduler-prefix ordering check, fixture hashes and backups under `C:\winvm\ix8v-20261109`. The initial deferred assertion assumed `\??\` without Windows' `*1` scheduler prefix. A corrected assertion against the live pending list confirmed file index 0 before folder index 2; the actual reboot then verified deletion. Initial check-7 attempts failed in the limited holder launcher. A diagnostic proved that assigning parsed JSON to the typed `$Config` parameter retained `System.String`, because PowerShell variable names ignore case. The final launcher uses a separate `$settings` variable. The corrected rerun passed and supplies check 7 above; failed attempts remain in VM backups.

PowerShell Parser.ParseFile passed. `git diff --check` passed. No full Cargo gate or PowerShell analyzer was run for these two new files; the real Windows scenarios provide runtime verification. The unrelated worker's shared-VM CI results are not evidence for this task.

Corrected contention rerun: use the main command above with `-Checks @('7')`, `-WorkDir C:\winvm\ix8v-20261109\contention-final` and `-Out C:\winvm\ix8v-20261109\contention-report.md`. Console driver exit 0. The Global holder reported `elevated=False`; install returned 0 in 1.36 s. The file-held install returned 4 in 60.06 s; remove returned 4 in 20.02 s. Install after release returned 0 in 0.03 s. Captured SCM configuration remained unchanged during contention.

For the remaining Settings deferred-notice check, start this holder in a second elevated PowerShell console after installing the current service:

```powershell
.\scripts\index-service-e2e.ps1 -Hold Mapped `
  -Target 'C:\Program Files\Navo\Index\navo-index.exe' `
  -Signal C:\winvm\ix8v-owner-hold
```

After the ready file appears, click Settings > Search > Remove in the normal app and read the deferred notice. Release the helper with `New-Item -ItemType File C:\winvm\ix8v-owner-hold.release`. Use a fresh Signal path for each attempt. The holder also exits after 240 seconds.

Final verification and cleanup:

- Final script SHA256: `39075D91C581756550B846F99C250533E752B255A5484595DC20AF770360101A`. Host and VM hashes match; this is the file used for the successful contention rerun. Final PowerShell Parser.ParseFile: PASS.
- Main matrix returned 1 for observed failures; the reboot phase returned 1 for the persistent parent and exit-code mismatch. Corrected contention rerun returned 0. All rollback assertions passed; owner-only actions are not presented as passes.
- The scheduler-prefix assertion was corrected against the live pending list before reboot. No file or folder was manually removed before the reboot observation. `schedule-order.log`: `file_index=0; folder_index=2`.
- After recording the failures, the leftover lock was archived before manual cleanup. Restore command: `Expand-Archive C:\winvm\ix8v-20261109\final-Navo.zip -DestinationPath "C:\Program Files"`. This cleanup prepares the shared VM and does not change either removal verdict.
- Final cleanup: `service_count=0; app_install=False; Navo_folder=False; Index_folder=False; ProgramData_Navo=False; navo_process_count=0; harness_tasks=0`; `FINAL CLEANUP PASS`.
- Open risks: the parent cleanup bug remains in the tested revision; the plan expects exit 1 instead of 7; the listed UAC, Settings and uninstall callback timing checks require the owner. No further reboot is pending from this run.
